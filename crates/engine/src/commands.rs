//! The command registry. Add a command: write a `fn(&mut Session, &Json) ->
//! Result<Json>`, add a row to `REGISTRY`, add a test. See `docs/contributing.md`.

use coursecraft_course::{
    Action, Align, Animation, Autofit, BackgroundAudio, BankDraw, Choice, CmpOp, Color, Condition,
    Course, CourseOutline, CropRect, Cue, DashStyle, DragItem, Easing, Event, FillStyle, FontFace,
    FontStyle, Glow, Glue, HotspotRegion, HotspotShape, Layer, Link, ListKind, Location, MAX_CUES,
    MAX_FONT_FACES, MAX_FONT_FAMILY_CHARS, MatchPair, MotionPath, Object, ObjectKind, Outline,
    OutlineProblem, OutlineQuestion, OutlineSlide, Paragraph, Point, Question, QuestionBank,
    QuestionKind, QuizSettings, RadioOption, Rect, RevisitMode, Scene, Scoring, SeekMode, Shadow,
    ShapeKind, ShapeText, Size, Slide, SlideTarget, StateDef, TableBorders, TableCell, TableRow,
    TextRun, TextRunPatch, TextStyle, Theme, Timing, Transition, Trigger, Trim, VAlign, Value,
    VariableDef, WebSource, align_paragraphs, apply_link_to_range, apply_style_to_range,
    apply_text_edit, course_strings, find_in_course, is_valid_link_url, normalize_table_fractions,
    paragraphs_for, parse_color, relative_luminance, replace_in_target, string_mut,
    table_cell_owners,
};
use serde::de::DeserializeOwned;
use serde_json::{Value as Json, json};
use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::{Command, EngineError, Result, Session};

mod csv_import;
#[cfg(test)]
pub(crate) use csv_import::{MAX_CSV_BYTES, MAX_CSV_ROWS, parse_csv};
mod styles;

const MAX_TEXT: usize = 4096;
const MAX_DURATION_MS: u32 = 24 * 60 * 60 * 1000;

/// Every command, in menu order.
#[must_use]
pub fn registry() -> &'static [Command] {
    REGISTRY
}

static REGISTRY: &[Command] = &[
    Command {
        id: "course.info",
        label: "Course Info",
        menu: "",
        shortcut: None,
        params: "{}",
        mutates: false,
        run: course_info,
    },
    Command {
        id: "course.map",
        label: "Course Map",
        menu: "",
        shortcut: None,
        params: "{}",
        mutates: false,
        run: course_map,
    },
    Command {
        id: "course.validate",
        label: "Check Course",
        menu: "File/Check Course",
        shortcut: None,
        params: "{}",
        mutates: false,
        run: course_validate,
    },
    Command {
        id: "course.audit",
        label: "Accessibility Check",
        menu: "",
        shortcut: None,
        params: "{}",
        mutates: false,
        run: course_audit,
    },
    Command {
        id: "course.update",
        label: "Course Properties",
        menu: "File/Course Properties",
        shortcut: None,
        params: r#"{"title"?: string, "id"?: string, "start_slide"?: id|null, "language"?: string|null}"#,
        mutates: true,
        run: course_update,
    },
    Command {
        id: "course.set_slide_size",
        label: "Slide Size",
        menu: "",
        shortcut: None,
        params: r#"{"w": number, "h": number, "mode": "scale"|"keep"}"#,
        mutates: true,
        run: course_set_slide_size,
    },
    Command {
        id: "course.set_title",
        label: "Course Title",
        menu: "",
        shortcut: None,
        params: r#"{"title": string}"#,
        mutates: true,
        run: course_set_title,
    },
    Command {
        id: "course.set_player",
        label: "Player Settings",
        menu: "Project/Player Settings",
        shortcut: None,
        params: r#"{"show_nav"?: bool, "show_menu"?: bool, "show_seekbar"?: bool, "show_speed"?: bool, "show_fullscreen"?: bool, "show_transcript"?: bool, "show_title"?: bool, "show_volume"?: bool, "show_captions"?: bool, "logo"?: string|null, "navigation"?: "free"|"restricted"|"locked", "next_requires"?: {"kind": "nothing"|"timeline_ends"|"variable", "name"?: string}}"#,
        mutates: true,
        run: course_set_player,
    },
    Command {
        id: "course.set_quiz",
        label: "Quiz Settings",
        menu: "Project/Quiz Settings",
        shortcut: None,
        params: r#"{"passing_percent": 0..=100|null}"#,
        mutates: true,
        run: course_set_quiz,
    },
    Command {
        id: "course.set_background_audio",
        label: "Set Background Audio",
        menu: "",
        shortcut: None,
        params: r#"{"src"?: string | null, "volume"?: number, "loop"?: bool}"#,
        mutates: true,
        run: course_set_background_audio,
    },
    Command {
        id: "theme.set_palette",
        label: "Set Theme Palette",
        menu: "",
        shortcut: None,
        params: r##"{"colors": [{"id"?: id, "name": string, "color": "#RRGGBB"}]}"##,
        mutates: true,
        run: styles::theme_set_palette,
    },
    Command {
        id: "theme.set_palette_color",
        label: "Change Palette Colour",
        menu: "",
        shortcut: None,
        params: r##"{"id": id, "color": "#RRGGBB", "recolor"?: bool}"##,
        mutates: true,
        run: styles::theme_set_palette_color,
    },
    Command {
        id: "course.recolor",
        label: "Replace Colour",
        menu: "",
        shortcut: None,
        params: r##"{"from": "#RRGGBB", "to": "#RRGGBB"}"##,
        mutates: true,
        run: styles::course_recolor,
    },
    Command {
        id: "text_style.create",
        label: "New Text Style",
        menu: "",
        shortcut: None,
        params: r#"{"name": string, "from_object"?: {"slide": id, "object": id}, "style"?: TextStyle, "paragraphs"?: Paragraph | null}"#,
        mutates: true,
        run: styles::text_style_create,
    },
    Command {
        id: "text_style.update",
        label: "Update Text Style",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "name"?: string, "style"?: TextStyle fields, "paragraphs"?: Paragraph | null}"#,
        mutates: true,
        run: styles::text_style_update,
    },
    Command {
        id: "text_style.delete",
        label: "Delete Text Style",
        menu: "",
        shortcut: None,
        params: r#"{"id": id}"#,
        mutates: true,
        run: styles::text_style_delete,
    },
    Command {
        id: "text_style.apply",
        label: "Apply Text Style",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "slide": id, "objects": id[], "clear_overrides"?: bool}"#,
        mutates: true,
        run: styles::text_style_apply,
    },
    Command {
        id: "course.add_finish_button",
        label: "Add Finish Button",
        menu: "",
        shortcut: None,
        params: "{}",
        mutates: true,
        run: course_add_finish_button,
    },
    Command {
        id: "course.set_theme",
        label: "Theme Settings",
        menu: "",
        shortcut: None,
        params: r#"{"accent"?: color, "background"?: color, "text"?: color, "font"?: string, "bar"?: color, "bar_text"?: color}"#,
        mutates: true,
        run: course_set_theme,
    },
    Command {
        id: "course.add_font",
        label: "Add Font",
        menu: "",
        shortcut: None,
        params: r#"{"src": "fonts/<file>.ttf", "family": string, "style": "regular"|"bold"|"italic"|"bold_italic"}"#,
        mutates: true,
        run: course_add_font,
    },
    Command {
        id: "course.remove_font",
        label: "Remove Font",
        menu: "",
        shortcut: None,
        params: r#"{"family": string, "style"?: style} or {"src": "fonts/<file>.ttf"}"#,
        mutates: true,
        run: course_remove_font,
    },
    Command {
        id: "course.export_xliff",
        label: "Export Translation",
        menu: "",
        shortcut: None,
        params: r#"{"target_language": string}"#,
        mutates: false,
        run: course_export_xliff,
    },
    Command {
        id: "course.import_xliff",
        label: "Import Translation",
        menu: "",
        shortcut: None,
        params: r#"{"xliff": string}"#,
        mutates: true,
        run: course_import_xliff,
    },
    Command {
        id: "edit.undo",
        label: "Undo",
        menu: "Edit/Undo",
        shortcut: Some("Ctrl+Z"),
        params: "{}",
        mutates: false,
        run: edit_undo,
    },
    Command {
        id: "edit.redo",
        label: "Redo",
        menu: "Edit/Redo",
        shortcut: Some("Ctrl+Y"),
        params: "{}",
        mutates: false,
        run: edit_redo,
    },
    Command {
        id: "scene.add",
        label: "New Scene",
        menu: "Insert/New Scene",
        shortcut: None,
        params: r#"{"id"?: id, "title"?: string}"#,
        mutates: true,
        run: scene_add,
    },
    Command {
        id: "scene.update",
        label: "Scene Properties",
        menu: "",
        shortcut: None,
        params: r#"{"scene": id, "title"?: string}"#,
        mutates: true,
        run: scene_update,
    },
    Command {
        id: "scene.delete",
        label: "Delete Scene",
        menu: "Edit/Delete Scene",
        shortcut: None,
        params: r#"{"scene": id}"#,
        mutates: true,
        run: scene_delete,
    },
    Command {
        id: "slide.add",
        label: "New Slide",
        menu: "Insert/New Slide",
        shortcut: Some("Ctrl+M"),
        params: r#"{"id"?: id, "scene"?: id, "after"?: slide id, "title"?: string}"#,
        mutates: true,
        run: slide_add,
    },
    Command {
        id: "slide.update",
        label: "Slide Properties",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "title"?: string, "duration_ms"?: u32, "revisit"?: "auto"|"reset"|"resume"}"#,
        mutates: true,
        run: slide_update,
    },
    Command {
        id: "slide.delete",
        label: "Delete Slide",
        menu: "Edit/Delete Slide",
        shortcut: None,
        params: r#"{"slide": id}"#,
        mutates: true,
        run: slide_delete,
    },
    Command {
        id: "slide.move",
        label: "Move Slide",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "scene"?: id, "index": usize}"#,
        mutates: true,
        run: slide_move,
    },
    Command {
        id: "slide.duplicate",
        label: "Duplicate Slide",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "after"?: slide id}"#,
        mutates: true,
        run: slide_duplicate,
    },
    Command {
        id: "slide.paste",
        label: "Paste Slide",
        menu: "",
        shortcut: None,
        params: r#"{"scene"?: id, "after"?: slide id, "slide": Slide}"#,
        mutates: true,
        run: slide_paste,
    },
    Command {
        id: "slide.set_duration",
        label: "Slide Duration",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "ms": u32}"#,
        mutates: true,
        run: slide_set_duration,
    },
    Command {
        id: "slide.set_background",
        label: "Slide Background",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "background": color|null}"#,
        mutates: true,
        run: slide_set_background,
    },
    Command {
        id: "slide.set_transition",
        label: "Slide Transition",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "transition": Transition|null}"#,
        mutates: true,
        run: slide_set_transition,
    },
    Command {
        id: "slide.set_player_overrides",
        label: "Slide Player Settings",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "next_requires"?: {"kind": "nothing"|"timeline_ends"|"variable", "name"?: string}|null, "show_nav"?: bool|null, "show_seekbar"?: bool|null, "show_menu"?: bool|null}"#,
        mutates: true,
        run: slide_set_player_overrides,
    },
    Command {
        id: "master.add",
        label: "New Master",
        menu: "",
        shortcut: None,
        params: r#"{"title"?: string}"#,
        mutates: true,
        run: master_add,
    },
    Command {
        id: "master.delete",
        label: "Delete Master",
        menu: "",
        shortcut: None,
        params: r#"{"master": id}"#,
        mutates: true,
        run: master_delete,
    },
    Command {
        id: "slide.set_master",
        label: "Apply Master",
        menu: "",
        shortcut: None,
        params: r#"{"slides": id[], "master": id|null}"#,
        mutates: true,
        run: slide_set_master,
    },
    Command {
        id: "bank.create",
        label: "New Question Bank",
        menu: "",
        shortcut: None,
        params: r#"{"title"?: string}"#,
        mutates: true,
        run: bank_create,
    },
    Command {
        id: "bank.update",
        label: "Question Bank Properties",
        menu: "",
        shortcut: None,
        params: r#"{"bank": id, "title"?: string}"#,
        mutates: true,
        run: bank_update,
    },
    Command {
        id: "bank.delete",
        label: "Delete Question Bank",
        menu: "",
        shortcut: None,
        params: r#"{"bank": id}"#,
        mutates: true,
        run: bank_delete,
    },
    Command {
        id: "bank.add_question",
        label: "New Bank Question",
        menu: "",
        shortcut: None,
        params: r#"{"bank": id, "kind"?: QuestionKind, "title"?: string}"#,
        mutates: true,
        run: bank_add_question,
    },
    Command {
        id: "bank.import_csv",
        label: "Import Questions from CSV",
        menu: "",
        shortcut: None,
        params: r#"{"csv": string, "bank"?: string}"#,
        mutates: true,
        run: csv_import::bank_import_csv,
    },
    Command {
        id: "bank.move_slide",
        label: "Move Slide to Bank",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "bank": id, "index"?: usize}"#,
        mutates: true,
        run: bank_move_slide,
    },
    Command {
        id: "bank.remove_slide",
        label: "Remove Bank Slide",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id}"#,
        mutates: true,
        run: bank_remove_slide,
    },
    Command {
        id: "draw.insert",
        label: "New Draw",
        menu: "",
        shortcut: None,
        params: r#"{"bank": id, "count": u32, "scene"?: id, "index"?: usize}"#,
        mutates: true,
        run: draw_insert,
    },
    Command {
        id: "draw.update",
        label: "Draw Properties",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "bank"?: id, "count"?: u32}"#,
        mutates: true,
        run: draw_update,
    },
    Command {
        id: "slide.add_from_layout",
        label: "New Slide from Layout",
        menu: "",
        shortcut: None,
        params: r#"{"layout": "blank"|"title"|"title_content"|"two_columns"|"picture_text"|"question", "after"?: slide id, "scene"?: id, "master"?: id}"#,
        mutates: true,
        run: slide_add_from_layout,
    },
    Command {
        id: "course.import_outline",
        label: "Slides from Outline",
        menu: "Insert/Slides from Outline…",
        shortcut: None,
        params: r#"{"outline": {"title"?: string|null, "scenes": [{"title": string, "slides": [{"title": string, "body"?: [string], "notes"?: string|null, "question"?: {"prompt": string, "choices": [{"text": string, "correct": bool}]}}]}]}, "after"?: slide id}"#,
        mutates: true,
        run: course_import_outline,
    },
    Command {
        id: "template.save",
        label: "Save as Template",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "title"?: string}"#,
        mutates: true,
        run: template_save,
    },
    Command {
        id: "template.add_slide",
        label: "New Slide from Template",
        menu: "",
        shortcut: None,
        params: r#"{"template": id, "after"?: slide id, "scene"?: id}"#,
        mutates: true,
        run: template_add_slide,
    },
    Command {
        id: "template.delete",
        label: "Delete Template",
        menu: "",
        shortcut: None,
        params: r#"{"template": id}"#,
        mutates: true,
        run: template_delete,
    },
    Command {
        id: "layer.add",
        label: "New Layer",
        menu: "Insert/New Layer",
        shortcut: None,
        params: r#"{"id"?: id, "slide": id, "name"?: string}"#,
        mutates: true,
        run: layer_add,
    },
    Command {
        id: "layer.update",
        label: "Layer Properties",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer": id, "name"?: string, "duration_ms"?: u32, "hide_other_layers"?: bool, "pause_base_timeline"?: bool, "prevent_base_clicks"?: bool, "hide_base_objects"?: bool, "hide_when_timeline_ends"?: bool, "allow_seek"?: "auto"|"yes"|"no", "revisit"?: "auto"|"reset"|"resume"}"#,
        mutates: true,
        run: layer_update,
    },
    Command {
        id: "layer.delete",
        label: "Delete Layer",
        menu: "Edit/Delete Layer",
        shortcut: None,
        params: r#"{"slide": id, "layer": id}"#,
        mutates: true,
        run: layer_delete,
    },
    Command {
        id: "layer.move",
        label: "Move Layer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer": id, "to": usize}"#,
        mutates: true,
        run: layer_move,
    },
    Command {
        id: "layer.duplicate",
        label: "Duplicate Layer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer": id}"#,
        mutates: true,
        run: layer_duplicate,
    },
    Command {
        id: "object.add",
        label: "Insert Object",
        menu: "",
        shortcut: None,
        params: r#"{"id"?: id, "slide": id, "layer"?: id, "kind": ObjectKind, "frame": Rect, "name"?: string, "timing"?: Timing}"#,
        mutates: true,
        run: object_add,
    },
    Command {
        id: "object.update",
        label: "Object Properties",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "name"?: string, "kind"?: ObjectKind, "timing"?: Timing, "initial_state"?: string, "focus_order"?: u32|null, "decorative"?: bool, "description"?: string|null}"#,
        mutates: true,
        run: object_update,
    },
    Command {
        id: "object.reorder",
        label: "Reorder Object",
        menu: "Arrange/Bring Forward",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "index": usize}"#,
        mutates: true,
        run: object_reorder,
    },
    Command {
        id: "object.align",
        label: "Align Objects",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "objects": id[], "edge": "left"|"center"|"right"|"top"|"middle"|"bottom", "to"?: "selection"|"slide"}"#,
        mutates: true,
        run: object_align,
    },
    Command {
        id: "object.distribute",
        label: "Distribute Objects",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "objects": id[], "axis": "horizontal"|"vertical"}"#,
        mutates: true,
        run: object_distribute,
    },
    Command {
        id: "object.match_size",
        label: "Match Size",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "objects": id[], "dimension": "width"|"height"|"both"}"#,
        mutates: true,
        run: object_match_size,
    },
    Command {
        id: "object.group",
        label: "Group",
        menu: "Arrange/Group",
        shortcut: Some("Ctrl+G"),
        params: r#"{"slide": id, "objects": id[]}"#,
        mutates: true,
        run: object_group,
    },
    Command {
        id: "object.ungroup",
        label: "Ungroup",
        menu: "Arrange/Ungroup",
        shortcut: Some("Ctrl+Shift+G"),
        params: r#"{"slide": id, "objects": id[]}"#,
        mutates: true,
        run: object_ungroup,
    },
    Command {
        id: "object.duplicate",
        label: "Duplicate",
        menu: "Edit/Duplicate",
        shortcut: Some("Ctrl+D"),
        params: r#"{"slide": id, "object"?: id, "objects"?: id[]}"#,
        mutates: true,
        run: object_duplicate,
    },
    Command {
        id: "object.paste",
        label: "Paste",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer"?: id, "objects": Object[], "triggers"?: Trigger[], "offset"?: number, "source_slide"?: id}"#,
        mutates: true,
        run: object_paste,
    },
    Command {
        id: "object.set_frame",
        label: "Move/Resize Object",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "frame": Rect}"#,
        mutates: true,
        run: object_set_frame,
    },
    Command {
        id: "object.set_rotation",
        label: "Rotate Object",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "degrees": number}"#,
        mutates: true,
        run: object_set_rotation,
    },
    Command {
        id: "object.flip",
        label: "Flip Object",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "axis": "horizontal"|"vertical"}"#,
        mutates: true,
        run: object_flip,
    },
    Command {
        id: "object.set_opacity",
        label: "Set Opacity",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "opacity": number}"#,
        mutates: true,
        run: object_set_opacity,
    },
    Command {
        id: "object.set_fill_style",
        label: "Set Fill Style",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "fill_style": FillStyle|null}"#,
        mutates: true,
        run: object_set_fill_style,
    },
    Command {
        id: "object.set_shadow",
        label: "Set Shadow",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "shadow": Shadow|null}"#,
        mutates: true,
        run: object_set_shadow,
    },
    Command {
        id: "object.set_glow",
        label: "Set Glow",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "glow": Glow|null}"#,
        mutates: true,
        run: object_set_glow,
    },
    Command {
        id: "object.apply_style",
        label: "Apply Style",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "source_slide": id, "source": id, "objects": id[]}"#,
        mutates: true,
        run: object_apply_style,
    },
    Command {
        id: "object.set_drag",
        label: "Set Drag",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "drag": DragSpec|null}"#,
        mutates: true,
        run: object_set_drag,
    },
    Command {
        id: "object.set_lottie",
        label: "Set Lottie Playback",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "loop"?: bool, "autoplay"?: bool, "speed"?: number}"#,
        mutates: true,
        run: object_set_lottie,
    },
    Command {
        id: "object.set_web_source",
        label: "Set Web Source",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "source": WebSource}"#,
        mutates: true,
        run: object_set_web_source,
    },
    Command {
        id: "object.set_input",
        label: "Set Input",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "patch": {variable?, placeholder?, multiline?, max_chars?, min?, max?, step?, orientation?, start_angle?, end_angle?, label?, options?}}"#,
        mutates: true,
        run: object_set_input,
    },
    Command {
        id: "object.set_marker",
        label: "Set Marker",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "patch": {title?, body?, runs?, paragraphs?, icon?, color?}}"#,
        mutates: true,
        run: object_set_marker,
    },
    Command {
        id: "object.set_panel",
        label: "Set Object Panel",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "panel": id|null}"#,
        mutates: true,
        run: object_set_panel,
    },
    Command {
        id: "object.set_scroll_panel",
        label: "Set Scrolling Panel",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "patch": {content_height?, bar?}}"#,
        mutates: true,
        run: object_set_scroll_panel,
    },
    Command {
        id: "object.set_timer",
        label: "Set Timer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "patch": {duration_ms?, format?, autostart?, style?}}"#,
        mutates: true,
        run: object_set_timer,
    },
    Command {
        id: "object.set_meter",
        label: "Set Progress Meter",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object"?: id, "objects"?: id[], "patch": {variable?, min?, max?, style?, show_value?}}"#,
        mutates: true,
        run: object_set_meter,
    },
    Command {
        id: "object.set_path",
        label: "Edit Path Points",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "points": Point[], "start_glue"?: {object, site}|null, "end_glue"?: {object, site}|null}"#,
        mutates: true,
        run: object_set_path,
    },
    Command {
        id: "object.set_crop",
        label: "Crop Image",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "crop": {x,y,w,h}|null, "frame"?: Rect}"#,
        mutates: true,
        run: object_set_crop,
    },
    Command {
        id: "object.set_captions",
        label: "Set Captions",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "captions": string|null}"#,
        mutates: true,
        run: object_set_captions,
    },
    Command {
        id: "object.set_media",
        label: "Set Media Options",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "volume"?: number, "trim"?: {"start_ms", "end_ms"?} | null}"#,
        mutates: true,
        run: object_set_media,
    },
    Command {
        id: "video.add_cue",
        label: "Add Cue Point",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "at_ms"?: number, "name"?: string, "pause"?: bool, "id"?: id}"#,
        mutates: true,
        run: video_add_cue,
    },
    Command {
        id: "video.set_cue",
        label: "Edit Cue Point",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "cue": id, "patch": {"at_ms"?: number, "name"?: string, "pause"?: bool}}"#,
        mutates: true,
        run: video_set_cue,
    },
    Command {
        id: "video.remove_cue",
        label: "Delete Cue Point",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "cue": id}"#,
        mutates: true,
        run: video_remove_cue,
    },
    Command {
        id: "video.add_question_cue",
        label: "Add Question at Cue",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "at_ms"?: number, "name"?: string}"#,
        mutates: true,
        run: video_add_question_cue,
    },
    Command {
        id: "object.add_state",
        label: "New State",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "state": StateDef}"#,
        mutates: true,
        run: object_add_state,
    },
    Command {
        id: "object.update_state",
        label: "Update State",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "state": StateDef}"#,
        mutates: true,
        run: object_update_state,
    },
    Command {
        id: "object.delete_state",
        label: "Delete State",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "name": string}"#,
        mutates: true,
        run: object_delete_state,
    },
    Command {
        id: "object.delete",
        label: "Delete Object",
        menu: "Edit/Delete",
        shortcut: Some("Delete"),
        params: r#"{"slide": id, "object": id}"#,
        mutates: true,
        run: object_delete,
    },
    Command {
        id: "object.set_animation",
        label: "Set Animation",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "which": "entrance"|"exit", "animation": Animation|null}"#,
        mutates: true,
        run: object_set_animation,
    },
    Command {
        id: "object.add_motion_path",
        label: "Add Motion Path",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "points"?: Point[], "duration_ms"?: u32, "easing"?: Easing, "relative"?: bool}"#,
        mutates: true,
        run: object_add_motion_path,
    },
    Command {
        id: "object.update_motion_path",
        label: "Update Motion Path",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "path": id, "points"?: Point[], "duration_ms"?: u32, "easing"?: Easing, "relative"?: bool}"#,
        mutates: true,
        run: object_update_motion_path,
    },
    Command {
        id: "object.remove_motion_path",
        label: "Remove Motion Path",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "path": id}"#,
        mutates: true,
        run: object_remove_motion_path,
    },
    Command {
        id: "text.set",
        label: "Set Text",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "text": string}"#,
        mutates: true,
        run: text_set,
    },
    Command {
        id: "text.style_range",
        label: "Style Text Range",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "start": usize, "end": usize, "bold"?: bool, "italic"?: bool, "underline"?: bool, "strikethrough"?: bool, "baseline"?: "normal"|"superscript"|"subscript", "letter_spacing"?: number, "size"?: number, "color"?: string, "font"?: string}"#,
        mutates: true,
        run: text_style_range,
    },
    Command {
        id: "text.set_paragraph",
        label: "Set Paragraph Style",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "align"?: "left"|"center"|"right", "valign"?: "top"|"middle"|"bottom"}"#,
        mutates: true,
        run: text_set_paragraph,
    },
    Command {
        id: "text.set_paragraphs",
        label: "Set Paragraph Formatting",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "start"?: usize, "end"?: usize, "list"?: "none"|"bullet"|"number", "level"?: 0..4, "line_spacing"?: 0.8..3, "space_before"?: 0..200, "space_after"?: 0..200}"#,
        mutates: true,
        run: text_set_paragraphs,
    },
    Command {
        id: "text.set_link",
        label: "Set Link",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "start"?: usize, "end"?: usize, "link": Link|null}"#,
        mutates: true,
        run: text_set_link,
    },
    Command {
        id: "object.set_autofit",
        label: "Set Autofit",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "autofit": "none"|"shrink"}"#,
        mutates: true,
        run: object_set_autofit,
    },
    Command {
        id: "table.set_cell",
        label: "Set Table Cell",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "row": usize, "column": usize, "text"?: string, "runs"?: TextRun[], "paragraphs"?: Paragraph[], "fill"?: Color|null, "valign"?: "top"|"middle"|"bottom"}"#,
        mutates: true,
        run: table_set_cell,
    },
    Command {
        id: "table.insert_row",
        label: "Insert Table Row",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "index"?: usize}"#,
        mutates: true,
        run: table_insert_row,
    },
    Command {
        id: "table.delete_row",
        label: "Delete Table Row",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "index": usize}"#,
        mutates: true,
        run: table_delete_row,
    },
    Command {
        id: "table.insert_column",
        label: "Insert Table Column",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "index"?: usize}"#,
        mutates: true,
        run: table_insert_column,
    },
    Command {
        id: "table.delete_column",
        label: "Delete Table Column",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "index": usize}"#,
        mutates: true,
        run: table_delete_column,
    },
    Command {
        id: "table.resize",
        label: "Resize Table",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "columns"?: number[], "rows"?: number[]}"#,
        mutates: true,
        run: table_resize,
    },
    Command {
        id: "table.merge",
        label: "Merge Table Cells",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "row": usize, "column": usize, "rows": usize, "columns": usize}"#,
        mutates: true,
        run: table_merge,
    },
    Command {
        id: "table.split",
        label: "Split Table Cell",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "row": usize, "column": usize}"#,
        mutates: true,
        run: table_split,
    },
    Command {
        id: "table.set_borders",
        label: "Set Table Borders",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "outer"?: Outline|null, "inner"?: Outline|null}"#,
        mutates: true,
        run: table_set_borders,
    },
    Command {
        id: "table.set_header_row",
        label: "Set Header Row",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "object": id, "header_row": bool}"#,
        mutates: true,
        run: table_set_header_row,
    },
    Command {
        id: "trigger.add",
        label: "New Trigger",
        menu: "Insert/Trigger",
        shortcut: None,
        params: r#"{"slide": id, "layer"?: id, "trigger": Trigger, "index"?: usize}"#,
        mutates: true,
        run: trigger_add,
    },
    Command {
        id: "trigger.update",
        label: "Update Trigger",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer"?: id, "index": usize, "trigger": Trigger}"#,
        mutates: true,
        run: trigger_update,
    },
    Command {
        id: "trigger.move",
        label: "Move Trigger",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer"?: id, "from": usize, "to": usize}"#,
        mutates: true,
        run: trigger_move,
    },
    Command {
        id: "trigger.delete",
        label: "Delete Trigger",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "layer"?: id, "index": usize}"#,
        mutates: true,
        run: trigger_delete,
    },
    Command {
        id: "resource.add",
        label: "Add Resource",
        menu: "",
        shortcut: None,
        params: r#"{"title": string, "target": {"kind": "url", "url": string}|{"kind": "file", "src": string}}"#,
        mutates: true,
        run: resource_add,
    },
    Command {
        id: "resource.update",
        label: "Edit Resource",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "title"?: string, "target"?: {"kind": "url", "url": string}|{"kind": "file", "src": string}}"#,
        mutates: true,
        run: resource_update,
    },
    Command {
        id: "resource.remove",
        label: "Remove Resource",
        menu: "",
        shortcut: None,
        params: r#"{"id": id}"#,
        mutates: true,
        run: resource_remove,
    },
    Command {
        id: "resource.move",
        label: "Move Resource",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "to": usize}"#,
        mutates: true,
        run: resource_move,
    },
    Command {
        id: "glossary.add",
        label: "Add Glossary Entry",
        menu: "",
        shortcut: None,
        params: r#"{"term": string, "definition"?: string}"#,
        mutates: true,
        run: glossary_add,
    },
    Command {
        id: "glossary.update",
        label: "Edit Glossary Entry",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "term"?: string, "definition"?: string}"#,
        mutates: true,
        run: glossary_update,
    },
    Command {
        id: "glossary.remove",
        label: "Remove Glossary Entry",
        menu: "",
        shortcut: None,
        params: r#"{"id": id}"#,
        mutates: true,
        run: glossary_remove,
    },
    Command {
        id: "glossary.move",
        label: "Move Glossary Entry",
        menu: "",
        shortcut: None,
        params: r#"{"id": id, "to": usize}"#,
        mutates: true,
        run: glossary_move,
    },
    Command {
        id: "variable.add",
        label: "New Variable",
        menu: "Project/Variables",
        shortcut: None,
        params: r#"{"name": string, "value": bool|number|string}"#,
        mutates: true,
        run: variable_add,
    },
    Command {
        id: "variable.set_value",
        label: "Set Variable Value",
        menu: "",
        shortcut: None,
        params: r#"{"name": string, "value": bool|number|string}"#,
        mutates: true,
        run: variable_set_value,
    },
    Command {
        id: "variable.delete",
        label: "Delete Variable",
        menu: "",
        shortcut: None,
        params: r#"{"name": string}"#,
        mutates: true,
        run: variable_delete,
    },
    Command {
        id: "interaction.insert",
        label: "Interaction",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "kind": "tabs"|"accordion"}"#,
        mutates: true,
        run: interaction_insert,
    },
    Command {
        id: "question.insert",
        label: "Question",
        menu: "Insert/Question",
        shortcut: None,
        params: r#"{"slide": id, "kind"?: QuestionKind}"#,
        mutates: true,
        run: question_insert,
    },
    Command {
        id: "results.insert",
        label: "Results Slide",
        menu: "Insert/Results Slide",
        shortcut: None,
        params: r#"{"after"?: slide id}"#,
        mutates: true,
        run: results_insert,
    },
    Command {
        id: "certificate.insert",
        label: "Certificate Slide",
        menu: "Insert/Certificate Slide",
        shortcut: None,
        params: r#"{"after"?: slide id}"#,
        mutates: true,
        run: certificate_insert,
    },
    Command {
        id: "question.add_submit_button",
        label: "Add Submit Button",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id}"#,
        mutates: true,
        run: question_add_submit_button,
    },
    Command {
        id: "question.add_feedback_layers",
        label: "Add Feedback Layers",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id}"#,
        mutates: true,
        run: question_add_feedback_layers,
    },
    Command {
        id: "question.set",
        label: "Set Question",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "question": Question}"#,
        mutates: true,
        run: question_set,
    },
    Command {
        id: "question.delete",
        label: "Delete Question",
        menu: "Edit/Delete Question",
        shortcut: None,
        params: r#"{"slide": id}"#,
        mutates: true,
        run: question_delete,
    },
    Command {
        id: "question.set_kind",
        label: "Set Question Kind",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "kind": QuestionKind}"#,
        mutates: true,
        run: question_set_kind,
    },
    Command {
        id: "question.add_accepted",
        label: "Add Accepted Answer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "text"?: string}"#,
        mutates: true,
        run: question_add_accepted,
    },
    Command {
        id: "question.remove_accepted",
        label: "Remove Accepted Answer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "index": number}"#,
        mutates: true,
        run: question_remove_accepted,
    },
    Command {
        id: "question.update_accepted",
        label: "Update Accepted Answer",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "index": number, "text": string}"#,
        mutates: true,
        run: question_update_accepted,
    },
    Command {
        id: "question.set_case_sensitive",
        label: "Set Case Sensitive",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "case_sensitive": bool}"#,
        mutates: true,
        run: question_set_case_sensitive,
    },
    Command {
        id: "question.add_pair",
        label: "Add Match Pair",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "left"?: string, "right"?: string}"#,
        mutates: true,
        run: question_add_pair,
    },
    Command {
        id: "question.remove_pair",
        label: "Remove Match Pair",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string}"#,
        mutates: true,
        run: question_remove_pair,
    },
    Command {
        id: "question.update_pair",
        label: "Update Match Pair",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string, "left"?: string, "right"?: string}"#,
        mutates: true,
        run: question_update_pair,
    },
    Command {
        id: "question.reorder_pair",
        label: "Reorder Match Pair",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string, "to": number}"#,
        mutates: true,
        run: question_reorder_pair,
    },
    Command {
        id: "question.reorder_sequence",
        label: "Reorder Sequence Item",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string, "to": number}"#,
        mutates: true,
        run: question_reorder_sequence,
    },
    Command {
        id: "question.set_hotspot_image",
        label: "Set Hotspot Image",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "image": string}"#,
        mutates: true,
        run: question_set_hotspot_image,
    },
    Command {
        id: "question.add_region",
        label: "Add Hotspot Region",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "shape"?: HotspotShape, "x"?: number, "y"?: number, "w"?: number, "h"?: number, "correct"?: bool}"#,
        mutates: true,
        run: question_add_region,
    },
    Command {
        id: "question.update_region",
        label: "Update Hotspot Region",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string, "shape"?: HotspotShape, "x"?: number, "y"?: number, "w"?: number, "h"?: number, "correct"?: bool}"#,
        mutates: true,
        run: question_update_region,
    },
    Command {
        id: "question.remove_region",
        label: "Remove Hotspot Region",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "id": string}"#,
        mutates: true,
        run: question_remove_region,
    },
    Command {
        id: "question.set_markers",
        label: "Set Hotspot Markers",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "markers": number}"#,
        mutates: true,
        run: question_set_markers,
    },
    Command {
        id: "question.add_drag_item",
        label: "Add Drag Item",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "item": string, "target": string}"#,
        mutates: true,
        run: question_add_drag_item,
    },
    Command {
        id: "question.update_drag_item",
        label: "Update Drag Item",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "item": string, "target": string}"#,
        mutates: true,
        run: question_update_drag_item,
    },
    Command {
        id: "question.remove_drag_item",
        label: "Remove Drag Item",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "item": string}"#,
        mutates: true,
        run: question_remove_drag_item,
    },
    Command {
        id: "question.set_drop_targets",
        label: "Set Drop Targets",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "targets": [string]}"#,
        mutates: true,
        run: question_set_drop_targets,
    },
    Command {
        id: "question.set_drag_options",
        label: "Set Drag Options",
        menu: "",
        shortcut: None,
        params: r#"{"slide": id, "snap_to_center"?: bool, "return_if_wrong"?: bool, "one_per_target"?: bool}"#,
        mutates: true,
        run: question_set_drag_options,
    },
    Command {
        id: "course.find",
        label: "Find and Replace",
        menu: "Edit/Find and Replace…",
        shortcut: Some("Ctrl+F"),
        params: r#"{"query": string, "case"?: bool, "whole_word"?: bool}"#,
        mutates: false,
        run: course_find,
    },
    Command {
        id: "course.replace",
        label: "Replace",
        menu: "",
        shortcut: None,
        params: r#"{"query": string, "replacement": string, "case"?: bool, "whole_word"?: bool, "only"?: [{"slide"?: id, "object"?: id, "layer"?: id, "field": string}]}"#,
        mutates: true,
        run: course_replace,
    },
];

// ---------------------------------------------------------------- params

fn bad(cmd: &str, message: impl Into<String>) -> EngineError {
    EngineError::BadParams {
        command: cmd.to_owned(),
        message: message.into(),
    }
}

fn obj<'a>(cmd: &str, p: &'a Json) -> Result<&'a serde_json::Map<String, Json>> {
    p.as_object()
        .ok_or_else(|| bad(cmd, "params must be a JSON object"))
}

fn opt<T: DeserializeOwned>(cmd: &str, p: &Json, key: &str) -> Result<Option<T>> {
    match obj(cmd, p)?.get(key) {
        None | Some(Json::Null) => Ok(None),
        Some(v) => serde_json::from_value(v.clone())
            .map(Some)
            .map_err(|e| bad(cmd, format!("`{key}`: {e}"))),
    }
}

fn req<T: DeserializeOwned>(cmd: &str, p: &Json, key: &str) -> Result<T> {
    opt(cmd, p, key)?.ok_or_else(|| bad(cmd, format!("missing `{key}`")))
}

fn text(cmd: &str, s: String, key: &str) -> Result<String> {
    if s.len() > MAX_TEXT {
        return Err(bad(cmd, format!("`{key}` is longer than {MAX_TEXT} bytes")));
    }
    Ok(s)
}

fn validate_id(cmd: &str, id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(bad(cmd, "id cannot be empty"));
    }
    if id.len() > 64 {
        return Err(bad(cmd, "id is longer than 64 characters"));
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(bad(
            cmd,
            format!("id {id:?} must contain only [A-Za-z0-9_-]"),
        ));
    }
    Ok(())
}

/// `prefix1`, `prefix2`, … : the first not already taken.
fn fresh_id<'a>(prefix: &str, taken: impl Iterator<Item = &'a str>) -> String {
    let taken: std::collections::BTreeSet<&str> = taken.collect();
    (1u64..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id.as_str()))
        .unwrap_or_else(|| prefix.to_owned())
}

fn slide_mut<'a>(s: &'a mut Session, id: &str) -> Result<&'a mut Slide> {
    s.course
        .slide_mut(id)
        .ok_or_else(|| EngineError::NotFound(format!("slide {id:?} does not exist")))
}

fn layer_mut<'a>(slide: &'a mut Slide, id: &str) -> Result<&'a mut Layer> {
    let sid = slide.id.clone();
    slide
        .layer_mut(id)
        .ok_or_else(|| EngineError::NotFound(format!("layer {id:?} is not on slide {sid:?}")))
}

/// Reject the edit if it took a valid course to an invalid one.
fn keep_valid(s: &Session, was_valid: bool) -> Result<()> {
    if !was_valid {
        return Ok(());
    }
    match s
        .course
        .validate()
        .into_iter()
        .find(|i| i.severity == coursecraft_course::Severity::Error)
    {
        Some(i) => Err(EngineError::Invalid(format!("{} ({})", i.message, i.path))),
        None => Ok(()),
    }
}

// ---------------------------------------------------------------- commands

fn course_info(s: &mut Session, _p: &Json) -> Result<Json> {
    Ok(json!({
        "title": s.course.title,
        "scenes": s.course.scenes.len(),
        "slides": s.course.slides().count(),
        "revision": s.revision,
        "can_undo": s.can_undo(),
        "can_redo": s.can_redo(),
    }))
}

fn course_map(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(coursecraft_course::course_map(&s.course))
        .map_err(|e| EngineError::Invalid(e.to_string()))
}

fn course_validate(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(s.course.validate()).map_err(|e| EngineError::Invalid(e.to_string()))
}

fn course_audit(s: &mut Session, _p: &Json) -> Result<Json> {
    serde_json::to_value(s.course.audit()).map_err(|e| EngineError::Invalid(e.to_string()))
}

fn course_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.update";
    let was_valid = s.course.is_playable();
    let map = obj(C, p)?;
    if let Some(title) = opt::<String>(C, p, "title")? {
        let title = text(C, title, "title")?;
        if title.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        s.course.title = title;
    }
    if let Some(id) = opt::<String>(C, p, "id")? {
        validate_id(C, &id)?;
        s.course.id = id;
    }
    if let Some(v) = map.get("start_slide") {
        match v {
            Json::Null => s.course.start_slide = None,
            Json::String(id) => {
                if id.trim().is_empty() {
                    return Err(bad(C, "start_slide cannot be empty"));
                }
                s.course.start_slide = Some(id.clone());
            }
            _ => return Err(bad(C, "`start_slide`: expected string or null")),
        }
    }
    if let Some(v) = map.get("language") {
        match v {
            Json::Null => s.course.language = None,
            Json::String(tag) => {
                let tag = tag.trim();
                if tag.is_empty() {
                    s.course.language = None;
                } else if coursecraft_course::is_language_tag_valid(tag) {
                    s.course.language = Some(tag.to_owned());
                } else {
                    return Err(bad(C, "language is not a simple BCP 47 tag"));
                }
            }
            _ => return Err(bad(C, "`language`: expected string or null")),
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn course_set_slide_size(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_slide_size";
    let was_valid = s.course.is_playable();
    let w: u32 = req(C, p, "w")?;
    let h: u32 = req(C, p, "h")?;
    let mode: String = req(C, p, "mode")?;
    let scale = match mode.as_str() {
        "scale" => true,
        "keep" => false,
        _ => return Err(bad(C, "`mode` must be \"scale\" or \"keep\"")),
    };
    if !crate::slide_size::size_in_bounds(w, h) {
        return Err(bad(C, "size must be 100..=7680 wide and 100..=4320 high"));
    }
    if !crate::slide_size::set_slide_size(&mut s.course, w, h, scale) {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "w": w, "h": h }))
}

fn course_set_title(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_title";
    let was_valid = s.course.is_playable();
    let title = text(C, req(C, p, "title")?, "title")?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.title = title;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn course_set_player(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_player";
    let was_valid = s.course.is_playable();
    if let Some(v) = opt::<bool>(C, p, "show_nav")? {
        s.course.player.show_nav = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_menu")? {
        s.course.player.show_menu = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_seekbar")? {
        s.course.player.show_seekbar = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_speed")? {
        s.course.player.show_speed = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_fullscreen")? {
        s.course.player.show_fullscreen = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_transcript")? {
        s.course.player.show_transcript = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_title")? {
        s.course.player.show_title = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_volume")? {
        s.course.player.show_volume = v;
    }
    if let Some(v) = opt::<bool>(C, p, "show_captions")? {
        s.course.player.show_captions = v;
    }
    match obj(C, p)?.get("logo") {
        None => {}
        Some(Json::Null) => s.course.player.logo = None,
        Some(Json::String(src)) => {
            let src = text(C, src.clone(), "logo")?;
            if !coursecraft_course::is_image_src(&src) {
                return Err(bad(C, "`logo` must be a safe path to an image file"));
            }
            s.course.player.logo = Some(src);
        }
        Some(_) => return Err(bad(C, "`logo`: expected string or null")),
    }
    if let Some(v) = opt::<coursecraft_course::NavMode>(C, p, "navigation")? {
        s.course.player.navigation = v;
    }
    if let Some(v) = opt::<coursecraft_course::NextRequirement>(C, p, "next_requires")? {
        check_next_requirement(s, C, &v)?;
        s.course.player.next_requires = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A variable rule must name an existing true/false variable.
fn check_next_requirement(
    s: &Session,
    cmd: &str,
    rule: &coursecraft_course::NextRequirement,
) -> Result<()> {
    let coursecraft_course::NextRequirement::Variable { name } = rule else {
        return Ok(());
    };
    let kind = coursecraft_course::builtin_variable_kind(name)
        .or_else(|| s.course.variable(name).map(|def| def.value.kind()));
    match kind {
        None => Err(bad(cmd, format!("variable {name:?} does not exist"))),
        Some(coursecraft_course::ValueKind::Bool) => Ok(()),
        Some(_) => Err(bad(cmd, format!("variable {name:?} is not true/false"))),
    }
}

fn course_set_quiz(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_quiz";
    let was_valid = s.course.is_playable();
    let map = obj(C, p)?;
    match map.get("passing_percent") {
        None => return Err(bad(C, "missing `passing_percent`")),
        Some(Json::Null) => {
            s.course.quiz = None;
        }
        Some(Json::Number(n)) => {
            let Some(val) = n.as_u64() else {
                return Err(bad(C, "`passing_percent` must be an integer 0..=100"));
            };
            if val > 100 {
                return Err(bad(C, "`passing_percent` must be 0..=100"));
            }
            #[allow(clippy::cast_possible_truncation)]
            let passing_percent = val as u8;
            s.course.quiz = Some(QuizSettings { passing_percent });
        }
        _ => return Err(bad(C, "`passing_percent`: expected integer or null")),
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn course_set_background_audio(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_background_audio";
    let was_valid = s.course.is_playable();
    let map = obj(C, p)?;
    let volume: Option<f32> = opt(C, p, "volume")?;
    if let Some(volume) = volume
        && (!volume.is_finite() || !(0.0..=1.0).contains(&volume))
    {
        return Err(bad(C, "`volume` must be finite and within 0..=1"));
    }
    let looping: Option<bool> = opt(C, p, "loop")?;

    match map.get("src") {
        Some(Json::Null) => s.course.background_audio = None,
        Some(Json::String(src)) => {
            let src = text(C, src.clone(), "src")?;
            let audio = s
                .course
                .background_audio
                .get_or_insert_with(|| BackgroundAudio {
                    src: String::new(),
                    volume: 0.5,
                    looping: true,
                });
            audio.src = src;
        }
        Some(_) => return Err(bad(C, "`src`: expected string or null")),
        None => {
            if s.course.background_audio.is_none() && (volume.is_some() || looping.is_some()) {
                return Err(EngineError::NotFound(
                    "course has no background audio".to_string(),
                ));
            }
        }
    }

    if let Some(audio) = &mut s.course.background_audio {
        if let Some(volume) = volume {
            audio.volume = volume;
        }
        if let Some(looping) = looping {
            audio.looping = looping;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn course_add_finish_button(s: &mut Session, _p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let last_slide_id = s
        .course
        .scenes
        .iter()
        .rev()
        .find_map(|scene| scene.slides.last())
        .map(|slide| slide.id.clone())
        .ok_or_else(|| EngineError::Invalid("course has no slides".into()))?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &last_slide_id)?;
    let button = add_finish_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

fn validate_color(cmd: &'static str, c: &Color) -> Result<()> {
    if !c.is_valid() {
        return Err(bad(cmd, format!("invalid color hex {:?}", c.0)));
    }
    Ok(())
}

fn course_set_theme(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.set_theme";
    let was_valid = s.course.is_playable();
    if let Some(accent) = opt::<String>(C, p, "accent")? {
        let col = Color(accent);
        validate_color(C, &col)?;
        s.course.theme.accent = col;
    }
    if let Some(bg) = opt::<String>(C, p, "background")? {
        let col = Color(bg);
        validate_color(C, &col)?;
        s.course.theme.background = col;
    }
    if let Some(text_col) = opt::<String>(C, p, "text")? {
        let col = Color(text_col);
        validate_color(C, &col)?;
        s.course.theme.text = col;
    }
    if let Some(font) = opt::<String>(C, p, "font")? {
        let f = text(C, font, "font")?;
        s.course.theme.font = f;
    }
    if let Some(bar) = opt::<String>(C, p, "bar")? {
        let col = Color(bar);
        validate_color(C, &col)?;
        s.course.theme.player.bar = col;
    }
    if let Some(bar_text) = opt::<String>(C, p, "bar_text")? {
        let col = Color(bar_text);
        validate_color(C, &col)?;
        s.course.theme.player.bar_text = col;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Trim and strip control characters from a font family name.
fn clean_font_family(raw: &str) -> String {
    raw.chars()
        .filter(|c| !c.is_control())
        .collect::<String>()
        .trim()
        .chars()
        .take(MAX_FONT_FAMILY_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn font_src_is_valid(src: &str) -> bool {
    let lower = src.to_ascii_lowercase();
    !src.is_empty()
        && src.starts_with("fonts/")
        && !src.starts_with('/')
        && !src.contains('\\')
        && !src.contains(':')
        && !src.chars().any(char::is_control)
        && src
            .split('/')
            .all(|segment| !segment.is_empty() && segment != "." && segment != "..")
        && (lower.ends_with(".ttf") || lower.ends_with(".otf"))
}

fn course_add_font(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.add_font";
    let was_valid = s.course.is_playable();
    let raw_family: String = req(C, p, "family")?;
    let family = clean_font_family(&raw_family);
    if family.is_empty() {
        return Err(bad(C, "`family` is empty"));
    }
    let style: FontStyle = req(C, p, "style")?;
    let src: String = req(C, p, "src")?;
    if !font_src_is_valid(&src) {
        return Err(bad(
            C,
            "`src` must be a .ttf or .otf media path under fonts/",
        ));
    }
    if s.course.fonts.len() >= MAX_FONT_FACES {
        return Err(bad(
            C,
            format!("a course may register at most {MAX_FONT_FACES} font faces"),
        ));
    }
    if s.course
        .fonts
        .iter()
        .any(|face| face.family.trim().eq_ignore_ascii_case(&family) && face.style == style)
    {
        return Err(bad(
            C,
            format!("font face for {family:?} {style:?} already exists"),
        ));
    }
    let face = FontFace { family, style, src };
    s.course.fonts.push(face.clone());
    keep_valid(s, was_valid)?;
    Ok(json!({ "font": face }))
}

fn course_remove_font(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.remove_font";
    let was_valid = s.course.is_playable();
    let family = opt::<String>(C, p, "family")?.map(|f| clean_font_family(&f));
    let style = opt::<FontStyle>(C, p, "style")?;
    let src = opt::<String>(C, p, "src")?;
    if family.as_deref().unwrap_or_default().is_empty() && src.is_none() {
        return Err(bad(C, "pass `family` or `src`"));
    }
    let before = s.course.fonts.len();
    s.course.fonts.retain(|face| {
        let family_matches = family
            .as_deref()
            .is_some_and(|name| face.family.trim().eq_ignore_ascii_case(name));
        let style_matches = style.is_none_or(|wanted| face.style == wanted);
        let src_matches = src.as_deref().is_some_and(|path| face.src == path);
        if family.is_some() {
            !(family_matches && style_matches)
        } else {
            !src_matches
        }
    });
    let removed = before.saturating_sub(s.course.fonts.len());
    if removed == 0 {
        return Err(EngineError::NotFound("font face not found".to_string()));
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "removed": removed }))
}

fn course_export_xliff(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.export_xliff";
    let target_language = req::<String>(C, p, "target_language")?;
    if !coursecraft_course::is_language_tag_valid(&target_language) {
        return Err(bad(C, "target_language is not a simple BCP 47 tag"));
    }
    let xliff = coursecraft_format::xliff::export_xliff(&s.course, &target_language)
        .map_err(|e| EngineError::Invalid(e.to_string()))?;
    Ok(Json::String(xliff))
}

fn course_import_xliff(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.import_xliff";
    let xliff = req::<String>(C, p, "xliff")?;
    let doc = coursecraft_format::xliff::parse_xliff(xliff.as_bytes())
        .map_err(|e| EngineError::Invalid(e.to_string()))?;
    let (course, report) = coursecraft_format::xliff::apply_xliff(&s.course, &doc);
    if let Some(issue) = course
        .validate()
        .into_iter()
        .find(|i| i.severity == coursecraft_course::Severity::Error)
    {
        return Err(EngineError::Invalid(format!(
            "translated course is invalid: {} ({})",
            issue.message, issue.path
        )));
    }
    s.course = course;
    Ok(json!({
        "applied": report.applied,
        "missing_target": report.missing_target,
        "source_changed": report.source_changed,
        "unknown_ids": report.unknown_ids,
        "placeholder_mismatch": report.placeholder_mismatch,
        "unsupported_script": report.unsupported_script,
    }))
}

/// Read-only search over every author-visible string.
fn course_find(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.find";
    let query = text(C, req::<String>(C, p, "query")?, "query")?;
    let case_sensitive = opt::<bool>(C, p, "case")?.unwrap_or(false);
    let whole_word = opt::<bool>(C, p, "whole_word")?.unwrap_or(false);
    let (hits, truncated) = find_in_course(&s.course, &query, case_sensitive, whole_word);
    let hits = serde_json::to_value(&hits).map_err(|e| EngineError::Invalid(e.to_string()))?;
    Ok(json!({ "hits": hits, "truncated": truncated }))
}

/// Replace in every author-visible string, or only in the `only` locations.
/// One undo step. Matches that overlap a `%name%` reference are skipped and
/// counted. A replace that changes nothing makes no undo step.
fn course_replace(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.replace";
    let was_valid = s.course.is_playable();
    let query = text(C, req::<String>(C, p, "query")?, "query")?;
    let replacement = text(C, req::<String>(C, p, "replacement")?, "replacement")?;
    let case_sensitive = opt::<bool>(C, p, "case")?.unwrap_or(false);
    let whole_word = opt::<bool>(C, p, "whole_word")?.unwrap_or(false);
    let only: Option<HashSet<Location>> =
        opt::<Vec<Location>>(C, p, "only")?.map(|list| list.into_iter().collect());

    let mut replaced = 0usize;
    let mut skipped = 0usize;
    if !query.is_empty() {
        let locations: Vec<Location> = course_strings(&s.course)
            .into_iter()
            .map(|item| item.location)
            .filter(|location| only.as_ref().is_none_or(|set| set.contains(location)))
            .collect();
        for location in &locations {
            let Some(target) = string_mut(&mut s.course, location) else {
                continue;
            };
            let outcome = replace_in_target(
                target,
                &query,
                &replacement,
                case_sensitive,
                whole_word,
                MAX_TEXT,
            );
            if outcome.too_long {
                return Err(bad(
                    C,
                    format!("the replacement would make a text longer than {MAX_TEXT} bytes"),
                ));
            }
            replaced = replaced.saturating_add(outcome.replaced);
            skipped = skipped.saturating_add(outcome.skipped);
        }
    }
    if replaced == 0 {
        s.skip_undo = true;
        return Ok(json!({ "replaced": 0, "skipped_variable_refs": skipped }));
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "replaced": replaced, "skipped_variable_refs": skipped }))
}

fn edit_undo(s: &mut Session, _p: &Json) -> Result<Json> {
    s.undo().map(|()| Json::Null)
}

fn edit_redo(s: &mut Session, _p: &Json) -> Result<Json> {
    s.redo().map(|()| Json::Null)
}

fn scene_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.add";
    let was_valid = s.course.is_playable();
    let id: Option<String> = opt(C, p, "id")?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if s.course.scenes.iter().any(|sc| sc.id == custom) {
                return Err(EngineError::Invalid(format!(
                    "scene id {custom:?} already exists"
                )));
            }
            custom
        }
        None => fresh_id("scene", s.course.scenes.iter().map(|sc| sc.id.as_str())),
    };
    let title =
        opt(C, p, "title")?.unwrap_or_else(|| format!("Scene {}", s.course.scenes.len() + 1));
    let title = text(C, title, "title")?;
    s.course.scenes.push(Scene {
        id: id.clone(),
        title,
        slides: Vec::new(),
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn scene_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "scene")?;
    let title: Option<String> = opt(C, p, "title")?;
    let scene = s
        .course
        .scenes
        .iter_mut()
        .find(|sc| sc.id == sid)
        .ok_or_else(|| EngineError::NotFound(format!("scene {sid:?} does not exist")))?;
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        scene.title = t;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn scene_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "scene.delete";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "scene")?;
    if s.course.scenes.len() <= 1 {
        return Err(EngineError::Invalid(
            "a course needs at least one scene".into(),
        ));
    }
    let pos = s
        .course
        .scenes
        .iter()
        .position(|sc| sc.id == sid)
        .ok_or_else(|| EngineError::NotFound(format!("scene {sid:?} does not exist")))?;
    s.course.scenes.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.add";
    let was_valid = s.course.is_playable();
    let scene: Option<String> = opt(C, p, "scene")?;
    let after: Option<String> = opt(C, p, "after")?;
    let id: Option<String> = opt(C, p, "id")?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if s.course.slides().any(|sl| sl.id == custom) {
                return Err(EngineError::Invalid(format!(
                    "slide id {custom:?} already exists"
                )));
            }
            custom
        }
        None => fresh_id("slide", s.course.all_slide_ids()),
    };
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| "Untitled".to_owned()),
        "title",
    )?;
    let new = Slide {
        id: id.clone(),
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: after
            .as_deref()
            .and_then(|after| s.course.play_slide(after))
            .and_then(|slide| slide.master.clone()),
        revisit: RevisitMode::Auto,
        player: None,
    };
    // Where: after a given slide, else at the end of the given scene, else of the last scene.
    let (si, pos) = if let Some(after) = &after {
        s.course
            .scenes
            .iter()
            .enumerate()
            .find_map(|(si, sc)| {
                sc.slides
                    .iter()
                    .position(|sl| sl.id == *after)
                    .map(|i| (si, i + 1))
            })
            .ok_or_else(|| EngineError::NotFound(format!("slide {after:?} does not exist")))?
    } else {
        let si = match &scene {
            Some(sc) => s
                .course
                .scenes
                .iter()
                .position(|x| x.id == *sc)
                .ok_or_else(|| EngineError::NotFound(format!("scene {sc:?} does not exist")))?,
            None => s
                .course
                .scenes
                .len()
                .checked_sub(1)
                .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
        };
        (si, s.course.scenes.get(si).map_or(0, |sc| sc.slides.len()))
    };
    let scene = s
        .course
        .scenes
        .get_mut(si)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    scene.slides.insert(pos.min(scene.slides.len()), new);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn slide_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let title: Option<String> = opt(C, p, "title")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let revisit: Option<RevisitMode> = opt(C, p, "revisit")?;
    let slide = slide_mut(s, &sid)?;
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        slide.title = t;
    }
    if let Some(ms) = duration_ms {
        if ms > MAX_DURATION_MS {
            return Err(bad(C, "duration is longer than 24 hours"));
        }
        slide.duration_ms = ms;
    }
    if let Some(mode) = revisit {
        slide.revisit = mode;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let id: String = req("slide.delete", p, "slide")?;
    if s.course.slides().count() <= 1 {
        return Err(EngineError::Invalid(
            "a course needs at least one slide".into(),
        ));
    }
    let mut found = false;
    for sc in &mut s.course.scenes {
        let before = sc.slides.len();
        sc.slides.retain(|sl| sl.id != id);
        found |= sc.slides.len() != before;
    }
    if !found {
        return Err(EngineError::NotFound(format!(
            "slide {id:?} does not exist"
        )));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.move";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let target_scene_id: Option<String> = opt(C, p, "scene")?;
    let index: usize = req(C, p, "index")?;

    if let Some(tid) = target_scene_id.as_deref()
        && !s.course.scenes.iter().any(|sc| sc.id == tid)
    {
        return Err(EngineError::NotFound(format!(
            "scene {tid:?} does not exist"
        )));
    }

    let mut found = None;
    for sc in &mut s.course.scenes {
        if let Some(pos) = sc.slides.iter().position(|sl| sl.id == sid) {
            let slide = sc.slides.remove(pos);
            found = Some((sc.id.clone(), slide));
            break;
        }
    }
    let (source_scene_id, slide) =
        found.ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;

    let target_id = target_scene_id.as_deref().unwrap_or(&source_scene_id);
    let target_scene = s
        .course
        .scenes
        .iter_mut()
        .find(|sc| sc.id == target_id)
        .ok_or_else(|| EngineError::NotFound(format!("scene {target_id:?} does not exist")))?;

    let at = index.min(target_scene.slides.len());
    target_scene.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.duplicate";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let after: Option<String> = opt(C, p, "after")?;
    let source = s
        .course
        .slide(&sid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    let copy = copy_slide(&s.course, source, &sid, false);
    let new_id = copy.slide.id.clone();
    let after = after.as_deref().or(Some(sid.as_str()));
    insert_slide_after(s, copy.slide, None, after)?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

fn slide_paste(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.paste";
    let was_valid = s.course.is_playable();
    let scene: Option<String> = opt(C, p, "scene")?;
    let after: Option<String> = opt(C, p, "after")?;
    let source: Slide = req(C, p, "slide")?;
    let old_id = source.id.clone();
    let copy = copy_slide(&s.course, source, &old_id, false);
    let new_id = copy.slide.id.clone();
    insert_slide_after(s, copy.slide, scene.as_deref(), after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

fn slide_set_duration(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let id: String = req("slide.set_duration", p, "slide")?;
    let ms: u32 = req("slide.set_duration", p, "ms")?;
    if ms > MAX_DURATION_MS {
        return Err(bad(
            "slide.set_duration",
            "duration is longer than 24 hours",
        ));
    }
    slide_mut(s, &id)?.duration_ms = ms;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_set_background(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_background";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let map = obj(C, p)?;
    let bg_val = map
        .get("background")
        .ok_or_else(|| bad(C, "missing `background`"))?;
    let bg = match bg_val {
        Json::Null => None,
        Json::String(hex) => {
            let col = Color(hex.clone());
            validate_color(C, &col)?;
            Some(col)
        }
        _ => return Err(bad(C, "`background` must be string or null")),
    };
    let sl = slide_mut(s, &sid)?;
    sl.background = bg;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_set_transition(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_transition";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let map = obj(C, p)?;
    let trans_val = map
        .get("transition")
        .ok_or_else(|| bad(C, "missing `transition`"))?;
    let transition: Option<Transition> = match trans_val {
        Json::Null => None,
        val => {
            let t: Transition = serde_json::from_value(val.clone())
                .map_err(|e| bad(C, format!("invalid transition: {e}")))?;
            if t.duration_ms < 1 || t.duration_ms > 60000 {
                return Err(bad(C, "transition duration_ms must be 1..=60000"));
            }
            Some(t)
        }
    };
    let sl = slide_mut(s, &sid)?;
    sl.transition = transition;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// An optional true/false override in a command's params: absent leaves it
/// alone, `null` clears it, a bool sets it.
fn flag_override(
    cmd: &str,
    map: &serde_json::Map<String, Json>,
    key: &str,
) -> Result<Option<Option<bool>>> {
    match map.get(key) {
        None => Ok(None),
        Some(Json::Null) => Ok(Some(None)),
        Some(Json::Bool(value)) => Ok(Some(Some(*value))),
        Some(_) => Err(bad(cmd, format!("`{key}`: expected true, false or null"))),
    }
}

fn slide_set_player_overrides(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_player_overrides";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let map = obj(C, p)?;
    let next_requires: Option<Option<coursecraft_course::NextRequirement>> = match map
        .get("next_requires")
    {
        None => None,
        Some(Json::Null) => Some(None),
        Some(v) => {
            let rule: coursecraft_course::NextRequirement = serde_json::from_value(v.clone())
                .map_err(|e| bad(C, format!("`next_requires`: {e}")))?;
            check_next_requirement(s, C, &rule)?;
            Some(Some(rule))
        }
    };
    let flags = [
        flag_override(C, map, "show_nav")?,
        flag_override(C, map, "show_seekbar")?,
        flag_override(C, map, "show_menu")?,
    ];
    let sl = slide_mut(s, &sid)?;
    let mut overrides = sl.player.take().unwrap_or_default();
    if let Some(rule) = next_requires {
        overrides.next_requires = rule;
    }
    if let Some(value) = flags[0] {
        overrides.show_nav = value;
    }
    if let Some(value) = flags[1] {
        overrides.show_seekbar = value;
    }
    if let Some(value) = flags[2] {
        overrides.show_menu = value;
    }
    sl.player = if overrides == coursecraft_course::SlidePlayerOverrides::default() {
        None
    } else {
        Some(overrides)
    };
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- masters

fn master_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "master.add";
    let was_valid = s.course.is_playable();
    let id = fresh_id("master", s.course.all_slide_ids());
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| format!("Master {}", s.course.masters.len() + 1)),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.masters.push(Slide {
        id: id.clone(),
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn master_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "master.delete";
    let was_valid = s.course.is_playable();
    let id: String = req(C, p, "master")?;
    let pos = s
        .course
        .masters
        .iter()
        .position(|master| master.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("master {id:?} does not exist")))?;
    s.course.masters.remove(pos);
    for scene in &mut s.course.scenes {
        for slide in &mut scene.slides {
            if slide.master.as_deref() == Some(id.as_str()) {
                slide.master = None;
            }
        }
    }
    for template in &mut s.course.templates {
        if template.master.as_deref() == Some(id.as_str()) {
            template.master = None;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn slide_set_master(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.set_master";
    let was_valid = s.course.is_playable();
    let slides: Vec<String> = req(C, p, "slides")?;
    let master: Option<String> = opt(C, p, "master")?;
    if slides.is_empty() {
        return Err(bad(C, "`slides` is empty"));
    }
    if let Some(ref master) = master
        && s.course.master(master).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "master {master:?} does not exist"
        )));
    }
    // Only play slides can take a master; a master cannot take another master.
    for id in &slides {
        if s.course.play_slide(id).is_none() {
            return Err(EngineError::NotFound(format!(
                "slide {id:?} does not exist"
            )));
        }
    }
    for scene in &mut s.course.scenes {
        for slide in &mut scene.slides {
            if slides.iter().any(|id| id == &slide.id) {
                slide.master = master.clone();
            }
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- banks and draws

fn bank_mut<'a>(s: &'a mut Session, id: &str) -> Result<&'a mut QuestionBank> {
    s.course
        .banks
        .iter_mut()
        .find(|bank| bank.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {id:?} does not exist")))
}

/// Total number of questions drawn from `bank_id` across all scene slides.
fn draw_total_for(s: &Session, bank_id: &str) -> usize {
    s.course
        .slides()
        .filter_map(|slide| slide.draw.as_ref())
        .filter(|draw| draw.bank == bank_id)
        .map(|draw| draw.count as usize)
        .sum()
}

fn bank_create(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.create";
    let was_valid = s.course.is_playable();
    let id = fresh_id("bank", s.course.banks.iter().map(|bank| bank.id.as_str()));
    let title = text(
        C,
        opt(C, p, "title")?
            .unwrap_or_else(|| format!("Question Bank {}", s.course.banks.len() + 1)),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    s.course.banks.push(QuestionBank {
        id: id.clone(),
        title,
        slides: Vec::new(),
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn bank_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.update";
    let was_valid = s.course.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let title: Option<String> = opt(C, p, "title")?;
    let bank = bank_mut(s, &bank_id)?;
    if let Some(t) = title {
        let t = text(C, t, "title")?;
        if t.trim().is_empty() {
            return Err(bad(C, "title is empty"));
        }
        bank.title = t;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn bank_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.delete";
    let was_valid = s.course.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let pos = s
        .course
        .banks
        .iter()
        .position(|bank| bank.id == bank_id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    if s.course
        .slides()
        .any(|slide| slide.draw.as_ref().is_some_and(|draw| draw.bank == bank_id))
    {
        return Err(EngineError::Invalid(format!(
            "bank {bank_id:?} is drawn from; remove those draws first"
        )));
    }
    s.course.banks.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A question slide as `question.insert` builds it, ready to put in a bank.
/// `customise` edits the question before the feedback layers are laid out.
fn new_bank_slide(
    course: &Course,
    kind: QuestionKind,
    title: String,
    customise: &dyn Fn(&mut Question),
) -> Result<Slide> {
    let id = fresh_id("slide", course.all_slide_ids());
    let theme = course.theme.clone();
    let slide_size = course.slide_size;
    let mut slide = Slide {
        id,
        title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };
    ensure_question_support(&mut slide, kind, &theme);
    let mut question = default_multiple_choice_question();
    apply_question_kind(&slide.objects, &mut question, kind)?;
    customise(&mut question);
    slide.question = Some(question);
    add_feedback_layers(&mut slide, &theme, slide_size);
    add_submit_button(&mut slide, &theme, slide_size);
    Ok(slide)
}

fn bank_add_question(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.add_question";
    let was_valid = s.course.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let kind: QuestionKind = opt(C, p, "kind")?.unwrap_or(QuestionKind::MultipleChoice);
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| "Untitled".to_owned()),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    let bank_pos = s
        .course
        .banks
        .iter()
        .position(|bank| bank.id == bank_id)
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    let slide = new_bank_slide(&s.course, kind, title, &|_| {})?;
    let id = slide.id.clone();
    let bank = s
        .course
        .banks
        .get_mut(bank_pos)
        .ok_or_else(|| EngineError::Invalid("bank index out of range".into()))?;
    bank.slides.push(slide);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn bank_move_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.move_slide";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id: String = req(C, p, "bank")?;
    let index: Option<usize> = opt(C, p, "index")?;
    if s.course.bank(&bank_id).is_none() {
        return Err(EngineError::NotFound(format!(
            "bank {bank_id:?} does not exist"
        )));
    }
    let mut found = None;
    for scene in &mut s.course.scenes {
        if let Some(pos) = scene.slides.iter().position(|slide| slide.id == sid) {
            let slide = scene.slides.remove(pos);
            found = Some(slide);
            break;
        }
    }
    let slide =
        found.ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    if slide.question.is_none() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} has no question; only question slides can move into a bank"
        )));
    }
    if slide.draw.is_some() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} is a draw slide; it cannot move into a bank"
        )));
    }
    let bank = bank_mut(s, &bank_id)?;
    let at = index.unwrap_or(bank.slides.len()).min(bank.slides.len());
    bank.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn bank_remove_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "bank.remove_slide";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id = s
        .course
        .banks
        .iter()
        .find(|bank| bank.slides.iter().any(|slide| slide.id == sid))
        .map(|bank| bank.id.clone())
        .ok_or_else(|| EngineError::NotFound(format!("bank slide {sid:?} does not exist")))?;
    let bank_size = s.course.bank(&bank_id).map_or(0, |bank| bank.slides.len());
    let remaining = bank_size.saturating_sub(1);
    let drawn = draw_total_for(s, &bank_id);
    if drawn > remaining {
        return Err(EngineError::Invalid(format!(
            "removing bank slide {sid:?} would leave draws asking for {drawn} questions from a bank of {remaining}"
        )));
    }
    let bank = bank_mut(s, &bank_id)?;
    bank.slides.retain(|slide| slide.id != sid);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn draw_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "draw.insert";
    let was_valid = s.course.is_playable();
    let bank_id: String = req(C, p, "bank")?;
    let count: u32 = req(C, p, "count")?;
    let scene_id: Option<String> = opt(C, p, "scene")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let bank_title = s
        .course
        .bank(&bank_id)
        .map(|bank| bank.title.clone())
        .ok_or_else(|| EngineError::NotFound(format!("bank {bank_id:?} does not exist")))?;
    if count == 0 {
        return Err(bad(C, "`count` must be at least 1"));
    }
    let id = fresh_id("slide", s.course.all_slide_ids());
    let slide = Slide {
        id: id.clone(),
        title: bank_title,
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: Some(BankDraw {
            bank: bank_id.clone(),
            count,
        }),
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };
    let scene_pos = match &scene_id {
        Some(scene) => s
            .course
            .scenes
            .iter()
            .position(|x| x.id == *scene)
            .ok_or_else(|| EngineError::NotFound(format!("scene {scene:?} does not exist")))?,
        None => s
            .course
            .scenes
            .len()
            .checked_sub(1)
            .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
    };
    let scene = s
        .course
        .scenes
        .get_mut(scene_pos)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    let at = index.unwrap_or(scene.slides.len()).min(scene.slides.len());
    scene.slides.insert(at, slide);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn draw_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "draw.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let bank_id: Option<String> = opt(C, p, "bank")?;
    let count: Option<u32> = opt(C, p, "count")?;
    if let Some(bank) = &bank_id
        && s.course.bank(bank).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "bank {bank:?} does not exist"
        )));
    }
    if let Some(count) = count
        && count == 0
    {
        return Err(bad(C, "`count` must be at least 1"));
    }
    let slide = slide_mut(s, &sid)?;
    let draw = slide
        .draw
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} is not a draw slide")))?;
    if let Some(bank) = bank_id {
        draw.bank = bank;
    }
    if let Some(count) = count {
        draw.count = count;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

// ---------------------------------------------------------------- layouts

const LAYOUT_MARGIN: f32 = 40.0;

fn layout_rect(x: f32, y: f32, w: f32, h: f32, size: Size) -> Rect {
    let slide_w = size.w.max(1) as f32;
    let slide_h = size.h.max(1) as f32;
    let x = if x.is_finite() {
        x.clamp(0.0, (slide_w - 1.0).max(0.0))
    } else {
        0.0
    };
    let y = if y.is_finite() {
        y.clamp(0.0, (slide_h - 1.0).max(0.0))
    } else {
        0.0
    };
    let w = if w.is_finite() { w } else { 1.0 };
    let h = if h.is_finite() { h } else { 1.0 };
    Rect {
        x,
        y,
        w: w.max(1.0).min((slide_w - x).max(1.0)),
        h: h.max(1.0).min((slide_h - y).max(1.0)),
    }
}

fn layout_text(
    id: String,
    name: &str,
    text: String,
    size: f32,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name: name.to_owned(),
        kind: ObjectKind::Text {
            text,
            style: coursecraft_course::TextStyle {
                font: theme.font.clone(),
                size,
                color: theme.text.clone(),
                ..coursecraft_course::TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
        },
        frame,
        timing: Timing::default(),
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
    }
}

/// What a layout placeholder is for. The layout chooses frames; this says which
/// placeholder the importer fills with an outline slide's title and body.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LayoutRole {
    Title,
    Content,
    /// Decorative or media placeholder, never filled with text.
    Media,
}

/// One placeholder of a starting layout: an object id prefix, the object name,
/// the role, and the frame.
struct Placeholder {
    prefix: &'static str,
    name: &'static str,
    role: LayoutRole,
    frame: Rect,
}

const PLACEHOLDER_TITLE: &str = "Click to add a title";
const PLACEHOLDER_TEXT: &str = "Click to add text";

/// The full list of layout names `slide.add_from_layout` accepts.
const LAYOUTS: [&str; 6] = [
    "blank",
    "title",
    "title_content",
    "two_columns",
    "picture_text",
    "question",
];

/// Placeholders of a starting layout, all inside the slide.
fn layout_placeholders(layout: &str, size: Size) -> Vec<Placeholder> {
    let slide_w = size.w.max(1) as f32;
    let slide_h = size.h.max(1) as f32;
    let left = LAYOUT_MARGIN.min(slide_w * 0.4);
    let top = LAYOUT_MARGIN.min(slide_h * 0.4);
    let inner_w = (slide_w - left * 2.0).max(1.0);
    let inner_h = (slide_h - top * 2.0).max(1.0);
    let gap = (inner_h * 0.04).clamp(8.0, 24.0).min(inner_h * 0.25);
    let title_h = (inner_h * 0.22).clamp(24.0, 90.0).min(inner_h);
    let body_y = top + title_h + gap;
    let body_h = (top + inner_h - body_y).max(1.0);
    let title = |y: f32| Placeholder {
        prefix: "text",
        name: "Title",
        role: LayoutRole::Title,
        frame: layout_rect(left, y, inner_w, title_h, size),
    };
    let content = |x: f32, w: f32, name: &'static str| Placeholder {
        prefix: "text",
        name,
        role: LayoutRole::Content,
        frame: layout_rect(x, body_y, w, body_h, size),
    };

    match layout {
        "title" => {
            let title_y = top + (inner_h - title_h).max(0.0) * 0.5;
            vec![title(title_y)]
        }
        "title_content" => vec![title(top), content(left, inner_w, "Text")],
        "two_columns" => {
            let col_gap = (inner_w * 0.04).clamp(8.0, 32.0).min(inner_w * 0.2);
            let col_w = ((inner_w - col_gap) * 0.5).max(1.0);
            vec![
                title(top),
                content(left, col_w, "Text"),
                content(left + col_w + col_gap, col_w, "Text 2"),
            ]
        }
        "picture_text" => {
            let col_gap = (inner_w * 0.04).clamp(8.0, 32.0).min(inner_w * 0.2);
            let image_w = ((inner_w - col_gap) * 0.45).max(1.0);
            let text_w = (inner_w - image_w - col_gap).max(1.0);
            vec![
                title(top),
                Placeholder {
                    prefix: "image",
                    name: "Picture",
                    role: LayoutRole::Media,
                    frame: layout_rect(left, body_y, image_w, body_h, size),
                },
                content(left + image_w + col_gap, text_w, "Text"),
            ]
        }
        _ => Vec::new(),
    }
}

/// Font size a layout placeholder starts with, by role.
fn layout_font_size(role: LayoutRole, frame: Rect) -> f32 {
    match role {
        LayoutRole::Title => 40.0_f32.min(frame.h.max(8.0)),
        _ => 20.0_f32,
    }
}

/// Objects for one of the six starting layouts, all inside the slide.
fn layout_objects(layout: &str, slide: &Slide, size: Size, theme: &Theme) -> Vec<Object> {
    let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
    let mut next = |prefix: &str| {
        let id = fresh_id_in(prefix, &taken);
        taken.insert(id.clone());
        id
    };
    layout_placeholders(layout, size)
        .into_iter()
        .map(|p| match p.role {
            LayoutRole::Media => Object {
                id: next(p.prefix),
                name: p.name.to_owned(),
                kind: ObjectKind::Image {
                    src: "placeholder.png".to_owned(),
                    alt: "Choose an image".to_owned(),
                    crop: None,
                },
                frame: p.frame,
                timing: Timing::default(),
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
            },
            role => layout_text(
                next(p.prefix),
                p.name,
                if role == LayoutRole::Title {
                    PLACEHOLDER_TITLE
                } else {
                    PLACEHOLDER_TEXT
                }
                .to_owned(),
                layout_font_size(role, p.frame),
                p.frame,
                theme,
            ),
        })
        .collect()
}

fn slide_add_from_layout(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "slide.add_from_layout";
    let was_valid = s.course.is_playable();
    let layout: String = req(C, p, "layout")?;
    if !LAYOUTS.contains(&layout.as_str()) {
        return Err(bad(
            C,
            "`layout` must be blank, title, title_content, two_columns, picture_text or question",
        ));
    }
    let after: Option<String> = opt(C, p, "after")?;
    let scene: Option<String> = opt(C, p, "scene")?;
    let master: Option<String> = opt(C, p, "master")?;
    if let Some(ref master) = master
        && s.course.master(master).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "master {master:?} does not exist"
        )));
    }
    let title = format!("Slide {}", s.course.slides().count().saturating_add(1));
    let mut params = json!({ "title": title });
    if let Some(ref after) = after {
        params["after"] = json!(after);
    }
    if let Some(ref scene) = scene {
        params["scene"] = json!(scene);
    }
    let added = slide_add(s, &params)?;
    let id = added
        .get("id")
        .and_then(Json::as_str)
        .ok_or_else(|| EngineError::Invalid("new slide has no id".into()))?
        .to_owned();
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    let new_id = {
        let slide = s
            .course
            .slide_mut(&id)
            .ok_or_else(|| EngineError::Invalid("new slide is missing".into()))?;
        if let Some(ref master) = master {
            slide.master = Some(master.clone());
        }
        let objects = layout_objects(&layout, slide, size, &theme);
        slide.objects.extend(objects);
        id.clone()
    };
    if layout == "question" {
        question_insert(s, &json!({ "slide": new_id, "kind": "multiple_choice" }))?;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

// ---------------------------------------------------------------- outline import

/// Turn an outline into slides in one undo step.
///
/// Everything happens here rather than by calling `scene.add` / `slide.add` /
/// `question.insert` so the whole import is a single `Session::execute` call:
/// one undo step, and any failure leaves the course exactly as it was (the
/// session also rolls back, but nothing is per-command undoable here).
fn course_import_outline(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.import_outline";
    let was_valid = s.course.is_playable();
    let outline: CourseOutline = req(C, p, "outline")?;
    let after: Option<String> = opt(C, p, "after")?;
    if let Some(ref after) = after
        && s.course.play_slide(after).is_none()
    {
        return Err(EngineError::NotFound(format!(
            "slide {after:?} does not exist"
        )));
    }
    let problems = outline.check();
    if !problems.is_empty() {
        return Err(EngineError::Invalid(describe_problems(&problems)));
    }
    // A checked outline with no slides is the dialog's empty field: nothing to
    // add, so no undo step rather than an empty one.
    if outline.slides().next().is_none() {
        s.skip_undo = true;
        return Ok(json!({ "scenes": 0, "slides": 0, "first_slide": null }));
    }

    let scene_at = insertion_point(s, after.as_deref())?;
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    // New slides take the master of the slide they follow: the last slide of
    // the scene just before the insertion point. With no `after`, that is the
    // course's last slide.
    let anchor_master = s
        .course
        .scenes
        .iter()
        .take(scene_at)
        .rev()
        .find_map(|scene| scene.slides.last())
        .and_then(|slide| slide.master.clone());

    let mut imported_scenes = 0usize;
    let mut imported_slides = 0usize;
    let mut first_slide: Option<String> = None;
    let mut taken: BTreeSet<String> = s.course.all_slide_ids().map(str::to_owned).collect();
    let mut taken_scenes: BTreeSet<String> =
        s.course.scenes.iter().map(|sc| sc.id.clone()).collect();
    let mut new_scenes = Vec::with_capacity(outline.scenes.len());
    for scene in &outline.scenes {
        let mut slides = Vec::with_capacity(scene.slides.len());
        for os in &scene.slides {
            let id = fresh_id_in("slide", &taken);
            taken.insert(id.clone());
            if first_slide.is_none() {
                first_slide = Some(id.clone());
            }
            slides.push(outline_slide(os, &id, &theme, size, anchor_master.clone()));
            imported_slides += 1;
        }
        if slides.is_empty() {
            continue;
        }
        imported_scenes += 1;
        let scene_id = fresh_id_in("scene", &taken_scenes);
        taken_scenes.insert(scene_id.clone());
        new_scenes.push(Scene {
            id: scene_id,
            title: scene.title.clone(),
            slides,
        });
    }
    let pos = scene_at.min(s.course.scenes.len());
    s.course.scenes.splice(pos..pos, new_scenes);
    keep_valid(s, was_valid)?;
    Ok(json!({
        "scenes": imported_scenes,
        "slides": imported_slides,
        "first_slide": first_slide,
    }))
}

/// One sentence listing the first few outline problems, then a count of the
/// rest. Outline problems carry no line numbers once parsed from JSON, so the
/// slide's own title says where to look.
fn describe_problems(problems: &[OutlineProblem]) -> String {
    const SHOWN: usize = 5;
    let mut message = match problems.len() {
        1 => "The outline has a problem: ".to_owned(),
        n => format!("The outline has {n} problems: "),
    };
    let shown: Vec<String> = problems
        .iter()
        .take(SHOWN)
        .map(|p| p.message.clone())
        .collect();
    message.push_str(&shown.join("; "));
    if problems.len() > SHOWN {
        message.push_str(&format!("; and {} more", problems.len() - SHOWN));
    }
    message
}

/// Where the import goes: the scene index the new scenes start at.
///
/// With `after`, the new scenes follow the scene that contains that slide, so
/// the imported slides stay together under their outline scene headings. With
/// no `after`, they follow every existing scene.
fn insertion_point(s: &Session, after: Option<&str>) -> Result<usize> {
    if let Some(after) = after {
        for (si, scene) in s.course.scenes.iter().enumerate() {
            if scene.slides.iter().any(|sl| sl.id == after) {
                return Ok(si + 1);
            }
        }
        return Err(EngineError::NotFound(format!(
            "slide {after:?} does not exist"
        )));
    }
    Ok(s.course.scenes.len())
}

/// Build one slide from an outline slide: the title layout or the
/// title-and-content layout, with notes and an optional graded question.
fn outline_slide(
    os: &OutlineSlide,
    id: &str,
    theme: &Theme,
    size: Size,
    master: Option<String>,
) -> Slide {
    // A question slide uses the question layout: the visible prompt is the
    // question, its body is ignored, and `question.insert`'s button and
    // feedback layers are added below.
    let layout = if os.question.is_some() {
        "question"
    } else if os.body.is_empty() {
        "title"
    } else {
        "title_content"
    };
    let mut slide = Slide {
        id: id.to_owned(),
        title: os.title.clone(),
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: os.notes.clone().unwrap_or_default(),
        master,
        revisit: RevisitMode::Auto,
        player: None,
    };
    // Fill the layout's own placeholders so the slide keeps one text object per
    // role (the title and, when there is body text, the content box).
    let placeholders = layout_placeholders(layout, size);
    let mut body_text = String::new();
    if os.question.is_none() {
        if os.body.len() >= 2 {
            for (i, line) in os.body.iter().enumerate() {
                if i > 0 {
                    body_text.push('\n');
                }
                body_text.push_str("• ");
                body_text.push_str(line);
            }
        } else if let Some(line) = os.body.first() {
            body_text.push_str(line);
        }
    }
    slide.objects = layout_objects(layout, &slide, size, theme);
    for (object, placeholder) in slide.objects.iter_mut().zip(placeholders) {
        let fill = match placeholder.role {
            LayoutRole::Title => &os.title,
            LayoutRole::Content => &body_text,
            LayoutRole::Media => continue,
        };
        if let ObjectKind::Text { text, runs, .. } = &mut object.kind {
            text.clone_from(fill);
            runs.clear();
        }
    }
    if let Some(q) = &os.question {
        slide.question = Some(outline_question(q));
        add_feedback_layers(&mut slide, theme, size);
        add_submit_button(&mut slide, theme, size);
    }
    slide
}

/// A multiple-choice question from an outline question. Choices get ids `a`,
/// `b`, `c`, … which are unique because ids only need to be unique per slide.
fn outline_question(q: &OutlineQuestion) -> Question {
    let mut base = default_multiple_choice_question();
    base.prompt = q.prompt.clone();
    base.choices = q
        .choices
        .iter()
        .enumerate()
        .map(|(i, c)| Choice {
            id: choice_id(i),
            text: c.text.clone(),
            correct: c.correct,
            points: 0,
        })
        .collect();
    base
}

/// `a`..`z`, then `a1`, `b1`, … — short, readable, unique.
fn choice_id(i: usize) -> String {
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
    let letter = LETTERS.get(i % LETTERS.len()).copied().unwrap_or(b'x');
    let round = i / LETTERS.len();
    let mut id = String::with_capacity(2);
    id.push(char::from(letter));
    if round > 0 {
        id.push_str(&round.to_string());
    }
    id
}

// ---------------------------------------------------------------- templates

fn template_save(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.save";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    if s.course.master(&sid).is_some() {
        return Err(bad(C, "a master cannot be saved as a template"));
    }
    let source = s
        .course
        .play_slide(&sid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
    let title = text(
        C,
        opt(C, p, "title")?.unwrap_or_else(|| source.title.clone()),
        "title",
    )?;
    if title.trim().is_empty() {
        return Err(bad(C, "title is empty"));
    }
    let id = fresh_id("template", s.course.all_slide_ids());
    let mut template = source;
    template.id = id.clone();
    template.title = title;
    s.course.templates.push(template);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn template_add_slide(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.add_slide";
    let was_valid = s.course.is_playable();
    let tid: String = req(C, p, "template")?;
    let after: Option<String> = opt(C, p, "after")?;
    let scene: Option<String> = opt(C, p, "scene")?;
    let source = s
        .course
        .template(&tid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("template {tid:?} does not exist")))?;
    let mut copy = copy_slide(&s.course, source, &tid, true);
    // A slide made from a template is a new slide, not a copy of one.
    copy.slide.title = format!("Slide {}", s.course.slides().count().saturating_add(1));
    let new_id = copy.slide.id.clone();
    insert_slide_after(s, copy.slide, scene.as_deref(), after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id, "skipped": copy.skipped }))
}

fn template_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "template.delete";
    let was_valid = s.course.is_playable();
    let tid: String = req(C, p, "template")?;
    let pos = s
        .course
        .templates
        .iter()
        .position(|template| template.id == tid)
        .ok_or_else(|| EngineError::NotFound(format!("template {tid:?} does not exist")))?;
    s.course.templates.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn layer_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.add";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: Option<String> = opt(C, p, "id")?;
    let name: Option<String> = opt(C, p, "name")?;
    let slide = slide_mut(s, &sid)?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if slide.local_ids().any(|existing| existing == custom) {
                return Err(EngineError::Invalid(format!(
                    "layer id {custom:?} already exists on slide {sid:?}"
                )));
            }
            custom
        }
        None => fresh_id("layer", slide.local_ids()),
    };
    let name = text(
        C,
        name.unwrap_or_else(|| format!("Layer {}", slide.layers.len() + 1)),
        "name",
    )?;
    slide.layers.push(Layer {
        id: id.clone(),
        name,
        duration_ms: 5000,
        objects: Vec::new(),
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn layer_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let name: Option<String> = opt(C, p, "name")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let hide_other_layers: Option<bool> = opt(C, p, "hide_other_layers")?;
    let pause_base_timeline: Option<bool> = opt(C, p, "pause_base_timeline")?;
    let prevent_base_clicks: Option<bool> = opt(C, p, "prevent_base_clicks")?;
    let hide_base_objects: Option<bool> = opt(C, p, "hide_base_objects")?;
    let hide_when_timeline_ends: Option<bool> = opt(C, p, "hide_when_timeline_ends")?;
    let allow_seek: Option<SeekMode> = opt(C, p, "allow_seek")?;
    let revisit: Option<RevisitMode> = opt(C, p, "revisit")?;

    let slide = slide_mut(s, &sid)?;
    let layer = layer_mut(slide, &lid)?;
    if let Some(n) = name {
        let n = text(C, n, "name")?;
        if n.trim().is_empty() {
            return Err(bad(C, "name is empty"));
        }
        layer.name = n;
    }
    if let Some(ms) = duration_ms {
        if ms > MAX_DURATION_MS {
            return Err(bad(C, "duration is longer than 24 hours"));
        }
        layer.duration_ms = ms;
    }
    if let Some(v) = hide_other_layers {
        layer.hide_other_layers = v;
    }
    if let Some(v) = pause_base_timeline {
        layer.pause_base_timeline = v;
    }
    if let Some(v) = prevent_base_clicks {
        layer.prevent_base_clicks = v;
    }
    if let Some(v) = hide_base_objects {
        layer.hide_base_objects = v;
    }
    if let Some(v) = hide_when_timeline_ends {
        layer.hide_when_timeline_ends = v;
    }
    if let Some(v) = allow_seek {
        layer.allow_seek = v;
    }
    if let Some(v) = revisit {
        layer.revisit = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn layer_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.delete";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let slide = slide_mut(s, &sid)?;
    let before = slide.layers.len();
    slide.layers.retain(|l| l.id != lid);
    if slide.layers.len() == before {
        return Err(EngineError::NotFound(format!(
            "layer {lid:?} is not on slide {sid:?}"
        )));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn layer_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.move";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;
    let to: usize = req(C, p, "to")?;

    let slide = slide_mut(s, &sid)?;
    let Some(pos) = slide.layers.iter().position(|l| l.id == lid) else {
        return Err(EngineError::NotFound(format!(
            "layer {lid:?} is not on slide {sid:?}"
        )));
    };
    // `slide.layers` runs back to front: index 0 is the backmost layer.
    let last = slide.layers.len().saturating_sub(1);
    let at = to.min(last);
    if at == pos {
        s.skip_undo = true;
        return Ok(Json::Null);
    }
    let layer = slide.layers.remove(pos);
    slide.layers.insert(at.min(slide.layers.len()), layer);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A name for a duplicated layer: `<name> copy`, then `<name> copy 2`, …
fn copied_layer_name(slide: &Slide, name: &str) -> String {
    let first = format!("{name} copy");
    if !slide.layers.iter().any(|layer| layer.name == first) {
        return first;
    }
    for n in 2u64..=u32::MAX as u64 {
        let candidate = format!("{name} copy {n}");
        if !slide.layers.iter().any(|layer| layer.name == candidate) {
            return candidate;
        }
    }
    first
}

fn layer_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "layer.duplicate";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let lid: String = req(C, p, "layer")?;

    // Build the copy against the slide as it is now, then insert it directly in
    // front of its source (a higher index is closer to the front).
    let (mut copy, insert_at, name, object_map, copied_map, layer_map, slide_id) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let (mut copy, index) = slide
            .layers
            .iter()
            .enumerate()
            .find(|(_, layer)| layer.id == lid)
            .map(|(index, layer)| (layer.clone(), index))
            .ok_or_else(|| {
                EngineError::NotFound(format!("layer {lid:?} is not on slide {sid:?}"))
            })?;

        let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        let mut taken_groups: BTreeSet<String> = all_group_ids(slide).into_iter().collect();
        let new_layer_id = fresh_id_in("layer", &taken);
        taken.insert(new_layer_id.clone());

        // References to anything outside the copy stay pointed at the same ids.
        let mut object_map: BTreeMap<String, String> = slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
            .map(|object| (object.id.clone(), object.id.clone()))
            .collect();
        let mut group_map: BTreeMap<String, String> = BTreeMap::new();
        let mut copied_map: BTreeMap<String, String> = BTreeMap::new();
        for object in &mut copy.objects {
            let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
            taken.insert(new_id.clone());
            object_map.insert(object.id.clone(), new_id.clone());
            copied_map.insert(object.id.clone(), new_id.clone());
            object.id = new_id;
            if let Some(old_group) = object.group.clone() {
                let new_group = group_map.entry(old_group).or_insert_with(|| {
                    let fresh = fresh_id_in("group", &taken_groups);
                    taken_groups.insert(fresh.clone());
                    fresh
                });
                object.group = Some(new_group.clone());
            }
        }

        let mut layer_map: BTreeMap<String, String> = slide
            .layers
            .iter()
            .map(|layer| (layer.id.clone(), layer.id.clone()))
            .collect();
        layer_map.insert(lid.clone(), new_layer_id.clone());

        let name = copied_layer_name(slide, &copy.name);
        (
            copy,
            index.saturating_add(1),
            name,
            object_map,
            copied_map,
            layer_map,
            slide.id.clone(),
        )
    };

    remap_object_glues(&mut copy.objects, &copied_map);
    let mut kept = Vec::with_capacity(copy.triggers.len());
    for mut trigger in std::mem::take(&mut copy.triggers) {
        if remap_trigger(
            &mut trigger,
            &object_map,
            &layer_map,
            &s.course,
            None,
            Some(&lid),
            false,
        ) {
            kept.push(trigger);
        }
    }
    copy.triggers = kept;
    let new_id = layer_map
        .get(&lid)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("layer {lid:?} disappeared")))?;
    copy.id = new_id.clone();
    copy.name = name;

    let slide = slide_mut(s, &slide_id)?;
    let at = insert_at.min(slide.layers.len());
    slide.layers.insert(at, copy);
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": new_id }))
}

// ---------------------------------------------------------------- copy/paste helpers

fn kind_prefix(kind: &ObjectKind) -> &'static str {
    match kind {
        ObjectKind::Text { .. } => "text",
        ObjectKind::Shape { .. } => "shape",
        ObjectKind::Image { .. } => "image",
        ObjectKind::Audio { .. } => "audio",
        ObjectKind::Video { .. } => "video",
        ObjectKind::Web { .. } => "web",
        ObjectKind::TextEntry { .. } => "text_entry",
        ObjectKind::Slider { .. } => "slider",
        ObjectKind::Dial { .. } => "dial",
        ObjectKind::Checkbox { .. } => "checkbox",
        ObjectKind::RadioGroup { .. } => "radio",
        ObjectKind::Marker { .. } => "marker",
        ObjectKind::ScrollPanel { .. } => "panel",
        ObjectKind::Timer { .. } => "timer",
        ObjectKind::ProgressMeter { .. } => "meter",
        ObjectKind::Lottie { .. } => "lottie",
        ObjectKind::Button { .. } => "button",
        ObjectKind::Path { .. } => "path",
        ObjectKind::Table { .. } => "table",
    }
}

fn fresh_id_in(prefix: &str, taken: &BTreeSet<String>) -> String {
    (1u64..)
        .map(|n| format!("{prefix}{n}"))
        .find(|id| !taken.contains(id))
        .unwrap_or_else(|| prefix.to_owned())
}

/// Keep at least one pixel of a pasted frame on the slide.
fn clamp_frame(frame: &mut Rect, size: Size) {
    let slide_w = size.w as f32;
    let slide_h = size.h as f32;
    if !frame.x.is_finite() {
        frame.x = 0.0;
    }
    if !frame.y.is_finite() {
        frame.y = 0.0;
    }
    if !frame.w.is_finite() || frame.w <= 0.0 {
        frame.w = 4.0;
    }
    if !frame.h.is_finite() || frame.h <= 0.0 {
        frame.h = 4.0;
    }
    if frame.w > slide_w.max(4.0) {
        frame.w = slide_w.max(4.0);
    }
    if frame.h > slide_h.max(4.0) {
        frame.h = slide_h.max(4.0);
    }
    let min_x = 1.0 - frame.w;
    let max_x = (slide_w - 1.0).max(min_x);
    let min_y = 1.0 - frame.h;
    let max_y = (slide_h - 1.0).max(min_y);
    frame.x = frame.x.clamp(min_x, max_x);
    frame.y = frame.y.clamp(min_y, max_y);
}

struct Remap<'a> {
    objects: &'a BTreeMap<String, String>,
    layers: &'a BTreeMap<String, String>,
    course: &'a Course,
    /// `(old, new)` when duplicating or pasting a whole slide.
    self_slide: Option<(&'a str, &'a str)>,
    /// Layer that owns the trigger being rewritten.
    owner_layer: Option<&'a str>,
    /// Keep a jump target that no longer exists so validation reports it.
    allow_missing_jump: bool,
}

fn remap_object(id: &mut String, r: &Remap<'_>) -> bool {
    match r.objects.get(id) {
        Some(new_id) => {
            *id = new_id.clone();
            true
        }
        None => false,
    }
}

fn remap_layer(id: &mut String, r: &Remap<'_>) -> bool {
    match r.layers.get(id) {
        Some(new_id) => {
            *id = new_id.clone();
            true
        }
        None => false,
    }
}

/// Keep a connector's glue only when its target was copied too, remapped to the
/// new id. Otherwise clear it; the end stays where it is.
fn remap_glue(glue: &mut Option<Glue>, objects: &BTreeMap<String, String>) {
    if let Some(current) = glue.as_mut() {
        match objects.get(&current.object) {
            Some(new_id) => current.object = new_id.clone(),
            None => *glue = None,
        }
    }
}

fn remap_object_glue(object: &mut Object, map: &BTreeMap<String, String>) {
    if let ObjectKind::Path { path } = &mut object.kind {
        remap_glue(&mut path.start_glue, map);
        remap_glue(&mut path.end_glue, map);
    }
}

fn remap_object_glues(objects: &mut [Object], map: &BTreeMap<String, String>) {
    for object in objects {
        remap_object_glue(object, map);
    }
}

fn variable_exists(course: &Course, name: &str) -> bool {
    coursecraft_course::builtin_variable_kind(name).is_some() || course.variable(name).is_some()
}

fn remap_event(event: &mut Event, r: &Remap<'_>) -> bool {
    match event {
        Event::TimelineStarts { layer }
        | Event::TimelineEnds { layer }
        | Event::TimelineReaches { layer, .. } => match layer {
            Some(id) => remap_layer(id, r),
            None => true,
        },
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MediaEnds { object }
        | Event::MarkerOpened { object }
        | Event::MarkerClosed { object } => remap_object(object, r),
        Event::CueReached { object, .. } => remap_object(object, r),
        Event::StateChanges { object, .. } | Event::AnimationCompletes { object, .. } => {
            remap_object(object, r)
        }
        Event::VariableChanges { variable } => variable_exists(r.course, variable),
        Event::KeyPressed { object, .. } => match object {
            Some(obj) => remap_object(obj, r),
            None => true,
        },
        Event::DroppedOn { object, target } => remap_object(object, r) && remap_object(target, r),
        Event::ObjectsIntersect { a, b } | Event::ObjectsStopIntersecting { a, b } => {
            remap_object(a, r) && remap_object(b, r)
        }
        Event::TimerEnds { timer } => remap_object(timer, r),
        Event::Submit => true,
    }
}

fn remap_action(action: &mut Action, r: &Remap<'_>) -> bool {
    match action {
        Action::JumpToSlide { target } => match target {
            SlideTarget::Next | SlideTarget::Previous => true,
            SlideTarget::Id(id) => {
                if let Some((old, new)) = r.self_slide
                    && id == old
                {
                    *id = new.to_owned();
                    return true;
                }
                r.allow_missing_jump || r.course.play_slide(id).is_some()
            }
        },
        Action::ShowLayer { layer } => remap_layer(layer, r),
        Action::HideLayer { layer } => match layer {
            Some(id) => remap_layer(id, r),
            None => r.owner_layer.is_some(),
        },
        Action::ChangeState { object, .. }
        | Action::PlayMedia { object }
        | Action::PauseMedia { object }
        | Action::StopMedia { object }
        | Action::StartTimer { timer: object }
        | Action::PauseTimer { timer: object }
        | Action::ResetTimer { timer: object }
        | Action::MoveOnPath { object, .. } => remap_object(object, r),
        Action::SetVariable { variable, .. } => variable_exists(r.course, variable),
        Action::PauseTimeline
        | Action::ResumeTimeline
        | Action::SubmitQuestion
        | Action::SubmitResults
        | Action::RetryQuiz
        | Action::ReviewQuiz
        | Action::ExitReview
        | Action::PrintSlide
        | Action::CompleteCourse { .. }
        | Action::ExitCourse
        | Action::OpenUrl { .. }
        | Action::RunJavaScript { .. }
        | Action::CloseLightbox => true,
        Action::LightboxSlide { slide } => {
            if let Some((old, new)) = r.self_slide
                && slide == old
            {
                *slide = new.to_owned();
                return true;
            }
            r.allow_missing_jump || r.course.play_slide(slide).is_some()
        }
    }
}

fn remap_condition(condition: &mut Condition, r: &Remap<'_>) -> bool {
    match condition {
        Condition::All(list) | Condition::Any(list) => {
            list.iter_mut().all(|inner| remap_condition(inner, r))
        }
        Condition::Not(inner) => remap_condition(inner, r),
        Condition::Compare { variable, .. } => variable_exists(r.course, variable),
        Condition::ObjectState { object, .. } => remap_object(object, r),
    }
}

fn remap_trigger(
    trigger: &mut Trigger,
    objects: &BTreeMap<String, String>,
    layers: &BTreeMap<String, String>,
    course: &Course,
    self_slide: Option<(&str, &str)>,
    owner_layer: Option<&str>,
    allow_missing_jump: bool,
) -> bool {
    let r = Remap {
        objects,
        layers,
        course,
        self_slide,
        owner_layer,
        allow_missing_jump,
    };
    if !remap_event(&mut trigger.when, &r) || !remap_action(&mut trigger.action, &r) {
        return false;
    }
    match &mut trigger.condition {
        Some(condition) => remap_condition(condition, &r),
        None => true,
    }
}

struct SlideCopy {
    slide: Slide,
    skipped: usize,
}

/// Give a slide, its layers and objects fresh ids, then rewrite every internal
/// reference. `self_slide` is the slide id the triggers originally used.
fn copy_slide(
    course: &Course,
    mut slide: Slide,
    self_slide: &str,
    allow_missing_jump: bool,
) -> SlideCopy {
    // Seed with the source ids so the copy gets genuinely fresh ids too.
    let mut taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
    let mut layer_map: BTreeMap<String, String> = BTreeMap::new();
    let mut object_map: BTreeMap<String, String> = BTreeMap::new();

    for layer in &mut slide.layers {
        let new_id = fresh_id_in("layer", &taken);
        taken.insert(new_id.clone());
        layer_map.insert(layer.id.clone(), new_id.clone());
        layer.id = new_id;
    }
    for object in &mut slide.objects {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        object_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id;
    }
    for layer in &mut slide.layers {
        for object in &mut layer.objects {
            let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
            taken.insert(new_id.clone());
            object_map.insert(object.id.clone(), new_id.clone());
            object.id = new_id;
        }
    }

    remap_object_glues(&mut slide.objects, &object_map);
    for layer in &mut slide.layers {
        remap_object_glues(&mut layer.objects, &object_map);
    }

    let new_slide_id = fresh_id("slide", course.all_slide_ids());
    let self_pair = (self_slide, new_slide_id.as_str());
    let mut skipped = 0;

    let mut kept = Vec::with_capacity(slide.triggers.len());
    for mut trigger in std::mem::take(&mut slide.triggers) {
        if remap_trigger(
            &mut trigger,
            &object_map,
            &layer_map,
            course,
            Some(self_pair),
            None,
            allow_missing_jump,
        ) {
            kept.push(trigger);
        } else {
            skipped += 1;
        }
    }
    slide.triggers = kept;

    for layer in &mut slide.layers {
        let owner = layer.id.clone();
        let mut kept = Vec::with_capacity(layer.triggers.len());
        for mut trigger in std::mem::take(&mut layer.triggers) {
            if remap_trigger(
                &mut trigger,
                &object_map,
                &layer_map,
                course,
                Some(self_pair),
                Some(&owner),
                allow_missing_jump,
            ) {
                kept.push(trigger);
            } else {
                skipped += 1;
            }
        }
        layer.triggers = kept;
    }

    if let Some(question) = &mut slide.question {
        if let Some(layer) = &mut question.correct_layer {
            *layer = layer_map.get(layer).cloned().unwrap_or_default();
            if layer.is_empty() {
                question.correct_layer = None;
            }
        }
        if let Some(layer) = &mut question.incorrect_layer {
            *layer = layer_map.get(layer).cloned().unwrap_or_default();
            if layer.is_empty() {
                question.incorrect_layer = None;
            }
        }
        if let Some(image) = &mut question.hotspot_image {
            *image = object_map.get(image).cloned().unwrap_or_default();
            if image.is_empty() {
                question.hotspot_image = None;
            }
        }
        question.drag_items.retain_mut(|drag| {
            match (object_map.get(&drag.item), object_map.get(&drag.target)) {
                (Some(item), Some(target)) => {
                    drag.item = item.clone();
                    drag.target = target.clone();
                    true
                }
                _ => false,
            }
        });
        let mut targets = Vec::with_capacity(question.drop_targets.len());
        for target in std::mem::take(&mut question.drop_targets) {
            if let Some(new_target) = object_map.get(&target)
                && !targets.contains(new_target)
            {
                targets.push(new_target.clone());
            }
        }
        question.drop_targets = targets;
    }

    slide.id = new_slide_id;
    slide.title = format!("{} copy", slide.title);

    SlideCopy { slide, skipped }
}

fn insert_slide_after(
    s: &mut Session,
    slide: Slide,
    scene: Option<&str>,
    after: Option<&str>,
) -> Result<()> {
    let (si, pos) = if let Some(after) = after {
        s.course
            .scenes
            .iter()
            .enumerate()
            .find_map(|(si, sc)| {
                sc.slides
                    .iter()
                    .position(|sl| sl.id == after)
                    .map(|i| (si, i + 1))
            })
            .ok_or_else(|| EngineError::NotFound(format!("slide {after:?} does not exist")))?
    } else {
        let si = match scene {
            Some(sc) => s
                .course
                .scenes
                .iter()
                .position(|x| x.id == sc)
                .ok_or_else(|| EngineError::NotFound(format!("scene {sc:?} does not exist")))?,
            None => s
                .course
                .scenes
                .len()
                .checked_sub(1)
                .ok_or_else(|| EngineError::Invalid("course has no scenes".into()))?,
        };
        (si, s.course.scenes.get(si).map_or(0, |sc| sc.slides.len()))
    };
    let target_scene = s
        .course
        .scenes
        .get_mut(si)
        .ok_or_else(|| EngineError::Invalid("scene index out of range".into()))?;
    let at = pos.min(target_scene.slides.len());
    target_scene.slides.insert(at, slide);
    Ok(())
}

fn object_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let mut kind: ObjectKind = req(C, p, "kind")?;
    clamp_shape_adjust(&mut kind);
    let frame: Rect = req(C, p, "frame")?;
    let name: Option<String> = opt(C, p, "name")?;
    let timing: Option<Timing> = opt(C, p, "timing")?;
    let id: Option<String> = opt(C, p, "id")?;
    let slide = slide_mut(s, &sid)?;
    let id = match id {
        Some(custom) => {
            validate_id(C, &custom)?;
            if slide.local_ids().any(|existing| existing == custom) {
                return Err(EngineError::Invalid(format!(
                    "object id {custom:?} already exists on slide {sid:?}"
                )));
            }
            custom
        }
        None => {
            let prefix = match kind {
                ObjectKind::Text { .. } => "text",
                ObjectKind::Shape { .. } => "shape",
                ObjectKind::Image { .. } => "image",
                ObjectKind::Audio { .. } => "audio",
                ObjectKind::Video { .. } => "video",
                ObjectKind::Web { .. } => "web",
                ObjectKind::TextEntry { .. } => "text_entry",
                ObjectKind::Slider { .. } => "slider",
                ObjectKind::Dial { .. } => "dial",
                ObjectKind::Checkbox { .. } => "checkbox",
                ObjectKind::RadioGroup { .. } => "radio",
                ObjectKind::Marker { .. } => "marker",
                ObjectKind::ScrollPanel { .. } => "panel",
                ObjectKind::Timer { .. } => "timer",
                ObjectKind::ProgressMeter { .. } => "meter",
                ObjectKind::Lottie { .. } => "lottie",
                ObjectKind::Button { .. } => "button",
                ObjectKind::Path { .. } => "path",
                ObjectKind::Table { .. } => "table",
            };
            fresh_id(prefix, slide.local_ids())
        }
    };
    let o = Object {
        id: id.clone(),
        name: text(C, name.unwrap_or_default(), "name")?,
        kind,
        frame,
        timing: timing.unwrap_or_default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
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
    match layer {
        Some(l) => layer_mut(slide, &l)?.objects.push(o),
        None => slide.objects.push(o),
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

/// Clamp shape adjustment values to the kind's limits before storing them.
fn clamp_shape_adjust(kind: &mut ObjectKind) {
    if let ObjectKind::Shape { shape, adjust, .. } = kind {
        *adjust = coursecraft_course::clamp_adjust(*shape, adjust);
    }
}

fn find_object_mut<'a>(slide: &'a mut Slide, id: &str) -> Result<&'a mut Object> {
    let sid = slide.id.clone();
    slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
        .find(|o| o.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}")))
}

/// True when the `kind` given to `object.update` has no `text_style` key.
fn kind_omits_text_style(p: &Json) -> bool {
    p.get("kind")
        .and_then(Json::as_object)
        .is_some_and(|k| !k.contains_key("text_style"))
}

/// A hand edit that leaves out `text_style` keeps the link to the named style.
fn keep_text_style_link(old: &ObjectKind, new: &mut ObjectKind, omitted: bool) {
    if !omitted {
        return;
    }
    if let (
        ObjectKind::Text {
            text_style: Some(link),
            ..
        },
        ObjectKind::Text {
            text_style: slot @ None,
            ..
        },
    ) = (old, new)
    {
        *slot = Some(link.clone());
    }
}

fn object_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: Option<String> = opt(C, p, "name")?;
    let kind: Option<ObjectKind> = opt(C, p, "kind")?.map(|mut kind| {
        clamp_shape_adjust(&mut kind);
        kind
    });
    let timing: Option<Timing> = opt(C, p, "timing")?;
    let initial_state: Option<String> = opt(C, p, "initial_state")?;
    let decorative: Option<bool> = opt(C, p, "decorative")?;
    let map = obj(C, p)?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    if let Some(n) = name {
        obj.name = text(C, n, "name")?;
    }
    if let Some(mut k) = kind {
        keep_text_style_link(&obj.kind, &mut k, kind_omits_text_style(p));
        obj.kind = k;
    }
    if let Some(t) = timing {
        obj.timing = t;
    }
    if let Some(is) = initial_state {
        let is = text(C, is, "initial_state")?;
        if is.trim().is_empty() {
            return Err(bad(C, "initial_state is empty"));
        }
        obj.initial_state = is;
    }
    if let Some(v) = map.get("focus_order") {
        obj.focus_order = match v {
            Json::Null => None,
            Json::Number(n) => {
                let Some(value) = n.as_u64().filter(|n| *n <= u64::from(u32::MAX)) else {
                    return Err(bad(
                        C,
                        "`focus_order` must be an unsigned 32-bit integer or null",
                    ));
                };
                #[allow(clippy::cast_possible_truncation)]
                Some(value as u32)
            }
            _ => {
                return Err(bad(
                    C,
                    "`focus_order`: expected an unsigned integer or null",
                ));
            }
        };
    }
    if let Some(v) = decorative {
        obj.decorative = v;
    }
    if let Some(v) = map.get("description") {
        obj.description = match v {
            Json::Null => None,
            Json::String(s) => {
                let s = text(C, s.clone(), "description")?;
                if s.trim().is_empty() { None } else { Some(s) }
            }
            _ => return Err(bad(C, "`description`: expected string or null")),
        };
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_reorder(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.reorder";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;

    let slide = slide_mut(s, &sid)?;
    let sid_str = slide.id.clone();
    if let Some(pos) = slide.objects.iter().position(|o| o.id == oid) {
        let obj = slide.objects.remove(pos);
        let at = index.min(slide.objects.len());
        slide.objects.insert(at, obj);
    } else {
        let mut found = false;
        for layer in &mut slide.layers {
            if let Some(pos) = layer.objects.iter().position(|o| o.id == oid) {
                let obj = layer.objects.remove(pos);
                let at = index.min(layer.objects.len());
                layer.objects.insert(at, obj);
                found = true;
                break;
            }
        }
        if !found {
            return Err(EngineError::NotFound(format!(
                "object {oid:?} is not on slide {sid_str:?}"
            )));
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Read and deduplicate an `objects` id list, rejecting an empty list.
fn objects_param(c: &str, p: &Json) -> Result<Vec<String>> {
    let objects: Vec<String> = req(c, p, "objects")?;
    let mut unique: Vec<String> = Vec::new();
    for id in objects {
        if !unique.contains(&id) {
            unique.push(id);
        }
    }
    if unique.is_empty() {
        return Err(bad(c, "`objects` is empty"));
    }
    Ok(unique)
}

fn finite_rect(rect: Rect) -> bool {
    [rect.x, rect.y, rect.w, rect.h]
        .iter()
        .all(|value| value.is_finite())
}

fn frame_bbox(rects: impl Iterator<Item = Rect>) -> Option<Rect> {
    let mut iter = rects;
    let first = iter.next()?;
    let (mut left, mut top) = (first.x, first.y);
    let (mut right, mut bottom) = (first.x + first.w, first.y + first.h);
    for rect in iter {
        left = left.min(rect.x);
        top = top.min(rect.y);
        right = right.max(rect.x + rect.w);
        bottom = bottom.max(rect.y + rect.h);
    }
    Some(Rect {
        x: left,
        y: top,
        w: right - left,
        h: bottom - top,
    })
}

/// One rigid unit for align and distribute: an ungrouped object, or every
/// listed member of the same group. Its bounding box is what moves.
struct LayoutUnit {
    members: Vec<(String, Rect)>,
    bbox: Rect,
}

fn layout_units(entries: Vec<(String, Rect, Option<String>)>) -> Vec<LayoutUnit> {
    type GroupedUnit = (Option<String>, Vec<(String, Rect)>);
    let mut grouped: Vec<GroupedUnit> = Vec::new();
    for (id, frame, group) in entries {
        if let Some(ref group) = group
            && let Some(unit) = grouped
                .iter_mut()
                .find(|(key, _)| key.as_deref() == Some(group.as_str()))
        {
            unit.1.push((id, frame));
            continue;
        }
        grouped.push((group, vec![(id, frame)]));
    }
    grouped
        .into_iter()
        .filter_map(|(_, members)| {
            let bbox = frame_bbox(members.iter().map(|(_, rect)| *rect))?;
            Some(LayoutUnit { members, bbox })
        })
        .collect()
}

fn object_align(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.align";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let edge: String = req(C, p, "edge")?;
    let to: Option<String> = opt(C, p, "to")?;
    let to = to.as_deref().unwrap_or("selection");
    if !matches!(
        edge.as_str(),
        "left" | "center" | "right" | "top" | "middle" | "bottom"
    ) {
        return Err(bad(
            C,
            "`edge` must be left, center, right, top, middle or bottom",
        ));
    }
    if !matches!(to, "selection" | "slide") {
        return Err(bad(C, "`to` must be selection or slide"));
    }

    let slide_size = s.course.slide_size;
    let (units, slide_rect) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries: Vec<(String, Rect, Option<String>)> = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame, object.group.clone()));
            }
        }
        let slide_rect = Rect {
            x: 0.0,
            y: 0.0,
            w: slide_size.w as f32,
            h: slide_size.h as f32,
        };
        (layout_units(entries), slide_rect)
    };
    if units.is_empty() {
        return Ok(json!({ "moved": 0 }));
    }
    let reference = if to == "slide" || units.len() == 1 {
        slide_rect
    } else {
        frame_bbox(units.iter().map(|unit| unit.bbox)).unwrap_or(slide_rect)
    };

    let mut updates: Vec<(String, Rect)> = Vec::new();
    for unit in &units {
        let bbox = unit.bbox;
        let (dx, dy) = match edge.as_str() {
            "left" => (reference.x - bbox.x, 0.0),
            "center" => (reference.x + reference.w * 0.5 - bbox.w * 0.5 - bbox.x, 0.0),
            "right" => (reference.x + reference.w - bbox.w - bbox.x, 0.0),
            "top" => (0.0, reference.y - bbox.y),
            "middle" => (0.0, reference.y + reference.h * 0.5 - bbox.h * 0.5 - bbox.y),
            "bottom" => (0.0, reference.y + reference.h - bbox.h - bbox.y),
            _ => (0.0, 0.0),
        };
        if !dx.is_finite() || !dy.is_finite() {
            continue;
        }
        for (id, frame) in &unit.members {
            let mut next = *frame;
            next.x += dx;
            next.y += dy;
            if finite_rect(next) && next != *frame {
                updates.push((id.clone(), next));
            }
        }
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

fn object_distribute(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.distribute";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let axis: String = req(C, p, "axis")?;
    if !matches!(axis.as_str(), "horizontal" | "vertical") {
        return Err(bad(C, "`axis` must be horizontal or vertical"));
    }

    let units: Vec<(usize, LayoutUnit)> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame, object.group.clone()));
            }
        }
        layout_units(entries).into_iter().enumerate().collect()
    };
    if units.len() < 3 {
        return Err(bad(C, "distribute needs at least 3 objects"));
    }

    let horizontal = axis == "horizontal";
    let mut sorted = units;
    sorted.sort_by(|a, b| {
        let (a_pos, b_pos) = if horizontal {
            (a.1.bbox.x, b.1.bbox.x)
        } else {
            (a.1.bbox.y, b.1.bbox.y)
        };
        a_pos.total_cmp(&b_pos).then(a.0.cmp(&b.0))
    });

    let Some((_, first)) = sorted.first() else {
        return Ok(json!({ "moved": 0 }));
    };
    let Some((_, last)) = sorted.last() else {
        return Ok(json!({ "moved": 0 }));
    };
    let (first_pos, first_size) = if horizontal {
        (first.bbox.x, first.bbox.w)
    } else {
        (first.bbox.y, first.bbox.h)
    };
    let (last_pos, last_size) = if horizontal {
        (last.bbox.x, last.bbox.w)
    } else {
        (last.bbox.y, last.bbox.h)
    };
    let total_span = (last_pos + last_size) - first_pos;
    let sum_sizes: f32 = sorted
        .iter()
        .map(|(_, unit)| if horizontal { unit.bbox.w } else { unit.bbox.h })
        .sum();
    #[allow(clippy::cast_precision_loss)]
    let gap = (total_span - sum_sizes) / (sorted.len() as f32 - 1.0);
    if !gap.is_finite() {
        return Ok(json!({ "moved": 0 }));
    }

    let mut updates: Vec<(String, Rect)> = Vec::new();
    let mut cursor = first_pos + first_size;
    let middle_end = sorted.len().saturating_sub(1);
    for (_, unit) in sorted.iter().take(middle_end).skip(1) {
        let target = cursor + gap;
        let delta = if horizontal {
            target - unit.bbox.x
        } else {
            target - unit.bbox.y
        };
        if !delta.is_finite() {
            continue;
        }
        for (id, frame) in &unit.members {
            let mut next = *frame;
            if horizontal {
                next.x += delta;
            } else {
                next.y += delta;
            }
            if finite_rect(next) && next != *frame {
                updates.push((id.clone(), next));
            }
        }
        cursor = target + if horizontal { unit.bbox.w } else { unit.bbox.h };
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

fn object_match_size(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.match_size";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let dimension: String = req(C, p, "dimension")?;
    if !matches!(dimension.as_str(), "width" | "height" | "both") {
        return Err(bad(C, "`dimension` must be width, height or both"));
    }

    let entries: Vec<(String, Rect)> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut entries = Vec::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if finite_rect(object.frame) {
                entries.push((id.clone(), object.frame));
            }
        }
        entries
    };
    let Some((_, primary)) = entries.first() else {
        return Ok(json!({ "moved": 0 }));
    };
    let (width, height) = (primary.w, primary.h);

    let mut updates: Vec<(String, Rect)> = Vec::new();
    for (id, frame) in entries.iter().skip(1) {
        let mut next = *frame;
        if matches!(dimension.as_str(), "width" | "both") {
            next.w = width;
        }
        if matches!(dimension.as_str(), "height" | "both") {
            next.h = height;
        }
        if finite_rect(next) && next != *frame {
            updates.push((id.clone(), next));
        }
    }
    let moved = updates.len();
    if moved > 0 {
        let slide = slide_mut(s, &sid)?;
        for (id, frame) in updates {
            find_object_mut(slide, &id)?.frame = frame;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "moved": moved }))
}

fn layer_objects_ref<'a>(slide: &'a Slide, layer: Option<&str>) -> Result<&'a [Object]> {
    match layer {
        Some(lid) => slide
            .layer(lid)
            .map(|layer| layer.objects.as_slice())
            .ok_or_else(|| {
                EngineError::NotFound(format!("layer {lid:?} is not on slide {:?}", slide.id))
            }),
        None => Ok(slide.objects.as_slice()),
    }
}

fn objects_vec_mut<'a>(slide: &'a mut Slide, layer: Option<&str>) -> Result<&'a mut Vec<Object>> {
    match layer {
        Some(lid) => Ok(&mut layer_mut(slide, lid)?.objects),
        None => Ok(&mut slide.objects),
    }
}

fn all_group_ids(slide: &Slide) -> Vec<String> {
    slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()))
        .filter_map(|object| object.group.clone())
        .collect()
}

fn object_group(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.group";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    if objects.len() < 2 {
        return Err(bad(C, "group needs at least 2 objects"));
    }

    let (layer, member_ids, insert_at, new_group) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut layer: Option<String> = None;
        let mut groups = BTreeSet::new();
        for (index, id) in objects.iter().enumerate() {
            let (object, owner) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            let owner = owner.map(str::to_owned);
            if index == 0 {
                layer = owner;
            } else if layer != owner {
                return Err(bad(C, "all objects must be on the same layer"));
            }
            if let Some(group) = &object.group {
                groups.insert(group.clone());
            }
        }
        let layer_objects = layer_objects_ref(slide, layer.as_deref())?;
        let selected: BTreeSet<&str> = objects.iter().map(String::as_str).collect();
        let member_ids: Vec<String> = layer_objects
            .iter()
            .filter(|object| {
                selected.contains(object.id.as_str())
                    || object
                        .group
                        .as_ref()
                        .is_some_and(|group| groups.contains(group))
            })
            .map(|object| object.id.clone())
            .collect();
        if member_ids.len() < 2 {
            return Err(bad(C, "group needs at least 2 objects"));
        }
        let topmost = member_ids
            .iter()
            .filter_map(|id| layer_objects.iter().position(|object| &object.id == id))
            .max()
            .ok_or_else(|| bad(C, "group members are not on this layer"))?;
        let insert_at = topmost.saturating_add(1).saturating_sub(member_ids.len());
        let taken = all_group_ids(slide);
        let new_group = fresh_id("group", taken.iter().map(String::as_str));
        (layer, member_ids, insert_at, new_group)
    };

    let member_set: BTreeSet<&str> = member_ids.iter().map(String::as_str).collect();
    let slide = slide_mut(s, &sid)?;
    let layer_objects = objects_vec_mut(slide, layer.as_deref())?;
    let mut extracted: Vec<Object> = Vec::new();
    let mut index = 0usize;
    while index < layer_objects.len() {
        if member_set.contains(layer_objects[index].id.as_str()) {
            extracted.push(layer_objects.remove(index));
        } else {
            index = index.saturating_add(1);
        }
    }
    for object in &mut extracted {
        object.group = Some(new_group.clone());
    }
    let count = extracted.len();
    let at = insert_at.min(layer_objects.len());
    for (offset, object) in extracted.into_iter().enumerate() {
        layer_objects.insert(at.saturating_add(offset), object);
    }

    keep_valid(s, was_valid)?;
    Ok(json!({ "group": new_group, "objects": count }))
}

fn object_ungroup(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.ungroup";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;

    let groups: BTreeSet<String> = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut groups = BTreeSet::new();
        for id in &objects {
            let (object, _) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            if let Some(group) = &object.group {
                groups.insert(group.clone());
            }
        }
        groups
    };
    if groups.is_empty() {
        return Ok(json!({ "objects": 0 }));
    }

    let mut count = 0usize;
    let slide = slide_mut(s, &sid)?;
    for object in slide.objects.iter_mut().chain(
        slide
            .layers
            .iter_mut()
            .flat_map(|layer| layer.objects.iter_mut()),
    ) {
        if object
            .group
            .as_ref()
            .is_some_and(|group| groups.contains(group))
        {
            object.group = None;
            count = count.saturating_add(1);
        }
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "objects": count }))
}

fn object_duplicate(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.duplicate";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let objects: Option<Vec<String>> = opt(C, p, "objects")?;
    let mut ids: Vec<String> = Vec::new();
    if let Some(objects) = objects {
        for id in objects {
            if !ids.contains(&id) {
                ids.push(id);
            }
        }
    } else {
        ids.push(req(C, p, "object")?);
    }
    if ids.is_empty() {
        return Err(bad(C, "`objects` is empty"));
    }

    let (sources, mut taken, mut taken_groups) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        let mut sources = Vec::new();
        for id in &ids {
            let (object, layer) = slide.object(id).ok_or_else(|| {
                EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
            })?;
            sources.push((object.clone(), layer.map(str::to_owned)));
        }
        let taken: BTreeSet<String> = slide.local_ids().map(str::to_owned).collect();
        let taken_groups: BTreeSet<String> = all_group_ids(slide).into_iter().collect();
        (sources, taken, taken_groups)
    };

    let mut group_map: BTreeMap<String, String> = BTreeMap::new();
    let mut copy_map: BTreeMap<String, String> = BTreeMap::new();
    let mut cloned: Vec<(Object, Option<String>)> = Vec::new();
    let mut new_ids: Vec<String> = Vec::new();
    for (mut object, layer) in sources {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        copy_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id.clone();
        object.frame.x += 20.0;
        object.frame.y += 20.0;
        if let Some(old_group) = object.group.clone() {
            let new_group = group_map.entry(old_group).or_insert_with(|| {
                let fresh = fresh_id_in("group", &taken_groups);
                taken_groups.insert(fresh.clone());
                fresh
            });
            object.group = Some(new_group.clone());
        }
        new_ids.push(new_id);
        cloned.push((object, layer));
    }
    for (object, _) in &mut cloned {
        remap_object_glue(object, &copy_map);
    }

    let slide = slide_mut(s, &sid)?;
    for (object, layer) in cloned {
        objects_vec_mut(slide, layer.as_deref())?.push(object);
    }

    keep_valid(s, was_valid)?;
    let first = new_ids.first().cloned().unwrap_or_default();
    Ok(json!({ "id": first, "ids": new_ids }))
}

fn object_paste(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.paste";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let objects: Vec<Object> = req(C, p, "objects")?;
    let triggers: Vec<Trigger> = opt(C, p, "triggers")?.unwrap_or_default();
    let offset: Option<f32> = opt(C, p, "offset")?;
    let source_slide: Option<String> = opt(C, p, "source_slide")?;

    if objects.is_empty() {
        return Err(bad(C, "`objects` is empty"));
    }
    if objects.len() > 500 || triggers.len() > 1000 {
        return Err(bad(C, "clipboard payload is too large"));
    }
    let offset = offset.unwrap_or(20.0);
    if !offset.is_finite() || offset.abs() > 100_000.0 {
        return Err(bad(C, "offset must be finite and within ±100000"));
    }
    for object in &objects {
        let frame = object.frame;
        if ![frame.x, frame.y, frame.w, frame.h]
            .iter()
            .all(|value| value.is_finite())
            || frame.w <= 0.0
            || frame.h <= 0.0
            || frame.x.abs() > 100_000.0
            || frame.y.abs() > 100_000.0
            || frame.w > 100_000.0
            || frame.h > 100_000.0
        {
            return Err(bad(
                C,
                "object frame values must be finite, positive and within ±100000",
            ));
        }
    }

    let slide_size = s.course.slide_size;
    let is_master = s.course.master(&sid).is_some();
    if is_master {
        if layer.is_some() {
            return Err(bad(C, "a master has no layers"));
        }
        if objects.iter().any(|object| {
            matches!(
                &object.kind,
                ObjectKind::Audio { .. } | ObjectKind::Video { .. }
            )
        }) {
            return Err(bad(C, "a master cannot hold audio or video"));
        }
    }
    let same_slide = source_slide.as_deref().is_none_or(|source| source == sid);
    let (mut taken, layers, mut taken_groups): (
        BTreeSet<String>,
        BTreeMap<String, String>,
        BTreeSet<String>,
    ) = {
        let slide = s
            .course
            .slide(&sid)
            .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} does not exist")))?;
        if let Some(ref lid) = layer
            && slide.layer(lid).is_none()
        {
            return Err(EngineError::NotFound(format!(
                "layer {lid:?} is not on slide {sid:?}"
            )));
        }
        (
            slide.local_ids().map(str::to_owned).collect(),
            if same_slide {
                slide
                    .layers
                    .iter()
                    .map(|l| (l.id.clone(), l.id.clone()))
                    .collect()
            } else {
                BTreeMap::new()
            },
            all_group_ids(slide).into_iter().collect(),
        )
    };

    let mut object_map: BTreeMap<String, String> = BTreeMap::new();
    let mut pasted: Vec<Object> = Vec::with_capacity(objects.len());
    let mut ids: Vec<String> = Vec::with_capacity(objects.len());

    for mut object in objects {
        let new_id = fresh_id_in(kind_prefix(&object.kind), &taken);
        taken.insert(new_id.clone());
        object_map.insert(object.id.clone(), new_id.clone());
        object.id = new_id.clone();
        object.frame.x += offset;
        object.frame.y += offset;
        clamp_frame(&mut object.frame, slide_size);
        ids.push(new_id);
        pasted.push(object);
    }

    remap_object_glues(&mut pasted, &object_map);

    let mut group_map: BTreeMap<String, String> = BTreeMap::new();
    for object in &mut pasted {
        if let Some(old_group) = object.group.clone() {
            let new_group = group_map.entry(old_group).or_insert_with(|| {
                let fresh = fresh_id_in("group", &taken_groups);
                taken_groups.insert(fresh.clone());
                fresh
            });
            object.group = Some(new_group.clone());
        }
    }

    let mut kept: Vec<Trigger> = Vec::with_capacity(triggers.len());
    let mut skipped = 0usize;
    for mut trigger in triggers {
        if is_master {
            // A master has no play-time behaviour, so its triggers are dropped.
            skipped += 1;
            continue;
        }
        let owner = if same_slide { layer.as_deref() } else { None };
        if remap_trigger(
            &mut trigger,
            &object_map,
            &layers,
            &s.course,
            None,
            owner,
            false,
        ) {
            kept.push(trigger);
        } else {
            skipped += 1;
        }
    }

    let slide = slide_mut(s, &sid)?;
    match &layer {
        Some(lid) => {
            let target = layer_mut(slide, lid)?;
            target.objects.extend(pasted);
            target.triggers.extend(kept);
        }
        None => {
            slide.objects.extend(pasted);
            slide.triggers.extend(kept);
        }
    }

    keep_valid(s, was_valid)?;
    let first = ids.first().cloned().unwrap_or_default();
    Ok(json!({ "id": first, "ids": ids, "skipped": skipped }))
}

fn object_set_frame(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let sid: String = req("object.set_frame", p, "slide")?;
    let oid: String = req("object.set_frame", p, "object")?;
    let frame: Rect = req("object.set_frame", p, "frame")?;
    find_object_mut(slide_mut(s, &sid)?, &oid)?.frame = frame;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Read `objects` when present, else the single `object` id. Deduplicated and
/// bounded, so one command can act on a multi-selection.
fn object_or_objects_param(c: &str, p: &Json) -> Result<Vec<String>> {
    if let Some(objects) = opt::<Vec<String>>(c, p, "objects")? {
        let mut unique: Vec<String> = Vec::new();
        for id in objects {
            if !unique.contains(&id) {
                unique.push(id);
            }
        }
        if unique.is_empty() {
            return Err(bad(c, "`objects` is empty"));
        }
        return Ok(unique);
    }
    Ok(vec![req(c, p, "object")?])
}

fn object_set_rotation(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_rotation";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let degrees: f32 = req(C, p, "degrees")?;
    if !degrees.is_finite() {
        return Err(bad(C, "`degrees` must be finite"));
    }
    let rotation = coursecraft_course::normalize_rotation(degrees);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.rotation != rotation {
                object.rotation = rotation;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "rotation": rotation, "changed": changed }))
}

fn object_flip(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.flip";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let axis: String = req(C, p, "axis")?;
    let horizontal = match axis.as_str() {
        "horizontal" => true,
        "vertical" => false,
        _ => return Err(bad(C, "`axis` must be horizontal or vertical")),
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if horizontal {
                object.flip_h = !object.flip_h;
                changed += 1;
            } else {
                object.flip_v = !object.flip_v;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

/// The appearance of one object, as copied by the format painter.
struct Look {
    opacity: f32,
    shadow: Option<Shadow>,
    glow: Option<Glow>,
    fill: Option<Color>,
    fill_style: Option<Option<FillStyle>>,
    outline: Option<Option<Outline>>,
    text_style: Option<TextStyle>,
}

fn look_of(object: &Object) -> Look {
    let mut look = Look {
        opacity: object.opacity,
        shadow: object.shadow.clone(),
        glow: object.glow.clone(),
        fill: None,
        fill_style: None,
        outline: None,
        text_style: None,
    };
    match &object.kind {
        ObjectKind::Text { style, .. } => look.text_style = Some(style.clone()),
        ObjectKind::Shape {
            fill,
            fill_style,
            outline,
            text,
            ..
        } => {
            look.fill = Some(fill.clone());
            look.fill_style = Some(fill_style.clone());
            look.outline = Some(outline.clone());
            look.text_style = text.as_ref().map(|t| t.style.clone());
        }
        ObjectKind::Button {
            fill, fill_style, ..
        } => {
            look.fill = Some(fill.clone());
            look.fill_style = Some(fill_style.clone());
        }
        _ => {}
    }
    look
}

fn clear_run_look(runs: &mut [TextRun]) {
    for run in runs {
        run.bold = None;
        run.italic = None;
        run.underline = None;
        run.strikethrough = None;
        run.baseline = None;
        run.letter_spacing = None;
        run.size = None;
        run.color = None;
        run.font = None;
    }
}

/// Returns true when anything changed.
fn apply_look(object: &mut Object, look: &Look) -> bool {
    let before = object.clone();
    object.opacity = look.opacity;
    object.shadow = look.shadow.clone();
    object.glow = look.glow.clone();
    match &mut object.kind {
        ObjectKind::Text { style, runs, .. } => {
            if let Some(src) = &look.text_style {
                *style = src.clone();
                clear_run_look(runs);
            }
        }
        ObjectKind::Shape {
            fill,
            fill_style,
            outline,
            text,
            ..
        } => {
            if let Some(src) = &look.fill {
                *fill = src.clone();
            }
            if let Some(src) = &look.fill_style {
                *fill_style = src.clone();
            }
            if let Some(src) = &look.outline {
                *outline = src.clone();
            }
            if let (Some(src), Some(t)) = (&look.text_style, text.as_mut()) {
                t.style = src.clone();
                clear_run_look(&mut t.runs);
            }
        }
        ObjectKind::Button {
            fill, fill_style, ..
        } => {
            if let Some(src) = &look.fill {
                *fill = src.clone();
            }
            if let Some(src) = &look.fill_style {
                *fill_style = src.clone();
            }
        }
        _ => {}
    }
    *object != before
}

fn object_apply_style(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.apply_style";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let source_slide: String = req(C, p, "source_slide")?;
    let source: String = req(C, p, "source")?;
    let ids = object_or_objects_param(C, p)?;
    let look = {
        let slide = s.course.slide(&source_slide).ok_or_else(|| {
            EngineError::NotFound(format!("slide {source_slide:?} does not exist"))
        })?;
        let (object, _) = slide.object(&source).ok_or_else(|| {
            EngineError::NotFound(format!(
                "object {source:?} is not on slide {source_slide:?}"
            ))
        })?;
        look_of(object)
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if apply_look(object, &look) {
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_opacity(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_opacity";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let opacity: f32 = req(C, p, "opacity")?;
    if !opacity.is_finite() {
        return Err(bad(C, "`opacity` must be finite"));
    }
    let opacity = coursecraft_course::clamp_opacity(opacity);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if (object.opacity - opacity).abs() > f32::EPSILON {
                object.opacity = opacity;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "opacity": opacity, "changed": changed }))
}

fn object_set_fill_style(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_fill_style";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let fill_style: Option<FillStyle> = opt(C, p, "fill_style")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let slot = match &mut object.kind {
                ObjectKind::Shape { fill_style, .. } | ObjectKind::Button { fill_style, .. } => {
                    Some(fill_style)
                }
                _ => None,
            };
            let Some(slot) = slot else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a shape or button")));
                }
                continue;
            };
            if *slot != fill_style {
                *slot = fill_style.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_shadow(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_shadow";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let shadow: Option<Shadow> = opt(C, p, "shadow")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.shadow != shadow {
                object.shadow = shadow.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_glow(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_glow";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let glow: Option<Glow> = opt(C, p, "glow")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.glow != glow {
                object.glow = glow.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_drag(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_drag";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let drag: Option<coursecraft_course::DragSpec> = opt(C, p, "drag")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            if object.drag != drag {
                object.drag = drag.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_lottie(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_lottie";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let loop_playback: Option<bool> = opt(C, p, "loop")?;
    let autoplay: Option<bool> = opt(C, p, "autoplay")?;
    let speed: Option<f32> = opt(C, p, "speed")?;
    if speed.is_some_and(|speed| !speed.is_finite()) {
        return Err(bad(C, "`speed` must be finite"));
    }
    let speed = speed.map(coursecraft_course::clamp_lottie_speed);
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let ObjectKind::Lottie {
                loop_playback: current_loop,
                autoplay: current_autoplay,
                speed: current_speed,
                ..
            } = &mut object.kind
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a Lottie object")));
                }
                continue;
            };
            let mut object_changed = false;
            if let Some(value) = loop_playback
                && *current_loop != value
            {
                *current_loop = value;
                object_changed = true;
            }
            if let Some(value) = autoplay
                && *current_autoplay != value
            {
                *current_autoplay = value;
                object_changed = true;
            }
            if let Some(value) = speed
                && (*current_speed - value).abs() > f32::EPSILON
            {
                *current_speed = value;
                object_changed = true;
            }
            if object_changed {
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_web_source(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_web_source";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let source: WebSource = req(C, p, "source")?;
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let ObjectKind::Web { source: current } = &mut object.kind else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a web object")));
                }
                continue;
            };
            if *current != source {
                *current = source.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

#[allow(clippy::cognitive_complexity)]
fn object_set_input(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_input";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let parse = |key: &str| -> Result<Option<Json>> { Ok(patch.get(key).cloned()) };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            match &mut next {
                ObjectKind::TextEntry {
                    variable,
                    placeholder,
                    multiline,
                    max_chars,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("placeholder")? {
                        *placeholder = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.placeholder: {e}")))?;
                    }
                    if let Some(value) = parse("multiline")? {
                        *multiline = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.multiline: {e}")))?;
                    }
                    if let Some(value) = parse("max_chars")? {
                        *max_chars = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max_chars: {e}")))?;
                    }
                }
                ObjectKind::Slider {
                    variable,
                    min,
                    max,
                    step,
                    orientation,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("min")? {
                        *min = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.min: {e}")))?;
                    }
                    if let Some(value) = parse("max")? {
                        *max = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max: {e}")))?;
                    }
                    if let Some(value) = parse("step")? {
                        *step = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.step: {e}")))?;
                    }
                    if let Some(value) = parse("orientation")? {
                        *orientation = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.orientation: {e}")))?;
                    }
                }
                ObjectKind::Dial {
                    variable,
                    min,
                    max,
                    step,
                    start_angle,
                    end_angle,
                } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("min")? {
                        *min = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.min: {e}")))?;
                    }
                    if let Some(value) = parse("max")? {
                        *max = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.max: {e}")))?;
                    }
                    if let Some(value) = parse("step")? {
                        *step = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.step: {e}")))?;
                    }
                    if let Some(value) = parse("start_angle")? {
                        *start_angle = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.start_angle: {e}")))?;
                    }
                    if let Some(value) = parse("end_angle")? {
                        *end_angle = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.end_angle: {e}")))?;
                    }
                }
                ObjectKind::Checkbox { variable, label } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("label")? {
                        *label = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.label: {e}")))?;
                    }
                }
                ObjectKind::RadioGroup { variable, options } => {
                    if let Some(value) = parse("variable")? {
                        *variable = serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
                    }
                    if let Some(value) = parse("options")? {
                        *options = serde_json::from_value::<Vec<RadioOption>>(value)
                            .map_err(|e| bad(C, format!("patch.options: {e}")))?;
                    }
                }
                _ => {
                    if single {
                        return Err(bad(C, format!("object {id:?} is not an input")));
                    }
                    continue;
                }
            }
            // Validate through the model's own repair and range rules.
            if let ObjectKind::Slider { min, max, .. } | ObjectKind::Dial { min, max, .. } =
                &mut next
            {
                *min = if min.is_finite() { *min } else { 0.0 };
                *max = if max.is_finite() { *max } else { 100.0 };
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_marker(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_marker";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let parse = |key: &str| -> Result<Option<Json>> { Ok(patch.get(key).cloned()) };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::Marker {
                title,
                body,
                runs,
                paragraphs,
                icon,
                color,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a marker")));
                }
                continue;
            };
            if let Some(value) = parse("title")? {
                *title = text(
                    C,
                    serde_json::from_value(value)
                        .map_err(|e| bad(C, format!("patch.title: {e}")))?,
                    "patch.title",
                )?;
            }
            if let Some(value) = parse("body")? {
                *body = text(
                    C,
                    serde_json::from_value(value)
                        .map_err(|e| bad(C, format!("patch.body: {e}")))?,
                    "patch.body",
                )?;
                if patch.get("runs").is_none() {
                    runs.clear();
                }
            }
            if let Some(value) = parse("runs")? {
                *runs = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.runs: {e}")))?;
            }
            if let Some(value) = parse("paragraphs")? {
                *paragraphs = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.paragraphs: {e}")))?;
            }
            if let Some(value) = parse("icon")? {
                *icon = serde_json::from_value(value)
                    .map_err(|e| bad(C, format!("patch.icon: {e}")))?;
            }
            if let Some(value) = parse("color")? {
                *color = match value {
                    Json::Null => None,
                    _ => Some(
                        serde_json::from_value(value)
                            .map_err(|e| bad(C, format!("patch.color: {e}")))?,
                    ),
                };
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_panel(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_panel";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let panel = match obj(C, p)?
        .get("panel")
        .ok_or_else(|| bad(C, "missing `panel`"))?
    {
        Json::Null => None,
        Json::String(id) => Some(id.clone()),
        _ => return Err(bad(C, "`panel` must be an id or null")),
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let layer = slide
                .object(id)
                .map(|(_, layer)| layer.map(str::to_owned))
                .ok_or_else(|| {
                    EngineError::NotFound(format!("object {id:?} is not on slide {sid:?}"))
                })?;
            if let Some(panel_id) = panel.as_deref() {
                if panel_id == id {
                    return Err(bad(C, "an object cannot be inside itself"));
                }
                let target = slide.object(panel_id).map(|(object, layer)| {
                    (
                        matches!(object.kind, ObjectKind::ScrollPanel { .. }),
                        layer.map(str::to_owned),
                    )
                });
                match target {
                    None => {
                        return Err(EngineError::NotFound(format!(
                            "panel {panel_id:?} is not on slide {sid:?}"
                        )));
                    }
                    Some((false, _)) => {
                        return Err(bad(
                            C,
                            format!("object {panel_id:?} is not a scrolling panel"),
                        ));
                    }
                    Some((true, target_layer)) if target_layer != layer => {
                        return Err(bad(
                            C,
                            format!("panel {panel_id:?} is on a different layer"),
                        ));
                    }
                    Some((true, _)) => {}
                }
            }
            let object = find_object_mut(slide, id)?;
            if panel.is_some() && matches!(object.kind, ObjectKind::ScrollPanel { .. }) {
                return Err(bad(
                    C,
                    format!("scrolling panel {id:?} cannot be inside another panel"),
                ));
            }
            if object.panel != panel {
                object.panel = panel.clone();
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_scroll_panel(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_scroll_panel";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::ScrollPanel {
                content_height,
                bar,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a scrolling panel")));
                }
                continue;
            };
            if let Some(value) = patch.get("content_height") {
                let value: f32 = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.content_height: {e}")))?;
                if !value.is_finite() {
                    return Err(bad(C, "patch.content_height must be finite"));
                }
                *content_height = value.clamp(
                    object.frame.h.max(0.0),
                    coursecraft_course::MAX_SCROLL_CONTENT_HEIGHT,
                );
            }
            if let Some(value) = patch.get("bar") {
                *bar = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.bar: {e}")))?;
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_timer(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_timer";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::Timer {
                duration_ms,
                format,
                autostart,
                style,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a timer")));
                }
                continue;
            };
            if let Some(value) = patch.get("duration_ms") {
                let value: i64 = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.duration_ms: {e}")))?;
                *duration_ms =
                    value.clamp(1, i64::from(coursecraft_course::MAX_TIMER_DURATION_MS)) as u32;
            }
            if let Some(value) = patch.get("format") {
                *format = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.format: {e}")))?;
            }
            if let Some(value) = patch.get("autostart") {
                *autostart = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.autostart: {e}")))?;
            }
            if let Some(value) = patch.get("style") {
                *style = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.style: {e}")))?;
                if !style.size.is_finite() || style.size <= 0.0 {
                    style.size = coursecraft_course::TextStyle::default().size;
                }
            }
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_meter(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_meter";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let ids = object_or_objects_param(C, p)?;
    let single = p.get("objects").is_none();
    let patch = p
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "`patch` must be an object"))?
        .clone();
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        for id in &ids {
            let object = find_object_mut(slide, id)?;
            let mut next = object.kind.clone();
            let ObjectKind::ProgressMeter {
                variable,
                min,
                max,
                style,
                show_value,
            } = &mut next
            else {
                if single {
                    return Err(bad(C, format!("object {id:?} is not a progress meter")));
                }
                continue;
            };
            if let Some(value) = patch.get("variable") {
                *variable = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.variable: {e}")))?;
            }
            if let Some(value) = patch.get("min") {
                *min = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.min: {e}")))?;
            }
            if let Some(value) = patch.get("max") {
                *max = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.max: {e}")))?;
            }
            if let Some(value) = patch.get("style") {
                *style = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.style: {e}")))?;
            }
            if let Some(value) = patch.get("show_value") {
                *show_value = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("patch.show_value: {e}")))?;
            }
            // Repair non-finite bounds the way loading does.
            *min = if min.is_finite() { *min } else { 0.0 };
            *max = if max.is_finite() { *max } else { 100.0 };
            if object.kind != next {
                object.kind = next;
                changed += 1;
            }
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn object_set_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_path";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let points: Vec<coursecraft_course::Point> = req(C, p, "points")?;
    if points.len() < coursecraft_course::PATH_MIN_POINTS
        || points.len() > coursecraft_course::PATH_MAX_POINTS
    {
        return Err(bad(
            C,
            format!(
                "`points` must have {} to {} entries",
                coursecraft_course::PATH_MIN_POINTS,
                coursecraft_course::PATH_MAX_POINTS
            ),
        ));
    }
    if points
        .iter()
        .any(|point| !point.x.is_finite() || !point.y.is_finite())
    {
        return Err(bad(C, "`points` must all be finite"));
    }
    let (frame, fractions) = coursecraft_course::path_frame_and_points(&points)
        .ok_or_else(|| bad(C, "`points` cannot form a path frame"))?;
    let params = obj(C, p)?;
    let start_present = params.contains_key("start_glue");
    let end_present = params.contains_key("end_glue");
    let start_glue: Option<Glue> = opt(C, p, "start_glue")?;
    let end_glue: Option<Glue> = opt(C, p, "end_glue")?;
    let object = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    match &mut object.kind {
        ObjectKind::Path { path } => {
            path.points = fractions;
            if start_present {
                path.start_glue = start_glue;
            }
            if end_present {
                path.end_glue = end_glue;
            }
            if path.connector.is_none() {
                path.start_glue = None;
                path.end_glue = None;
            }
        }
        _ => return Err(bad(C, format!("object {oid:?} is not a path"))),
    }
    object.frame = frame;
    keep_valid(s, was_valid)?;
    Ok(json!({ "frame": frame }))
}

fn object_set_crop(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_crop";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let raw = obj(C, p)?
        .get("crop")
        .ok_or_else(|| bad(C, "missing `crop`"))?;
    let crop: Option<CropRect> = match raw {
        Json::Null => None,
        value => Some(
            serde_json::from_value(value.clone()).map_err(|e| bad(C, format!("`crop`: {e}")))?,
        ),
    };
    if let Some(rect) = crop {
        check_crop(rect)?;
    }
    // A crop that keeps the whole picture is stored as no crop, so it draws and
    // saves exactly like one.
    let crop = crop.filter(|rect| !rect.is_full());
    let frame: Option<Rect> = opt(C, p, "frame")?;
    if let Some(frame) = frame
        && (![frame.x, frame.y, frame.w, frame.h]
            .iter()
            .all(|v| v.is_finite())
            || frame.w < 0.0
            || frame.h < 0.0)
    {
        return Err(bad(C, "`frame` values must be finite and w/h non-negative"));
    }

    let object = find_object_mut(slide_mut(s, &sid)?, &oid)?;
    match &mut object.kind {
        ObjectKind::Image {
            crop: current_crop, ..
        } => *current_crop = crop,
        _ => return Err(bad(C, format!("object {oid:?} is not an image"))),
    }
    if let Some(frame) = frame {
        object.frame = frame;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Reject a crop that is not a positive, finite window inside the source picture.
fn check_crop(rect: CropRect) -> Result<()> {
    const C: &str = "object.set_crop";
    let (x, y, w, h) = (rect.x, rect.y, rect.w, rect.h);
    if ![x, y, w, h].iter().all(|v| v.is_finite()) {
        return Err(bad(C, "`crop` values must be finite"));
    }
    if x < 0.0 || y < 0.0 || x > 1.0 || y > 1.0 || w > 1.0 || h > 1.0 {
        return Err(bad(C, "`crop` x, y, w and h must be fractions in 0..=1"));
    }
    if w <= 0.0 || h <= 0.0 {
        return Err(bad(C, "`crop` w and h must be greater than 0"));
    }
    if x + w > 1.000_001 || y + h > 1.000_001 {
        return Err(bad(
            C,
            "`crop` must lie inside the source (x + w, y + h <= 1)",
        ));
    }
    Ok(())
}

fn object_set_captions(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_captions";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let value = obj(C, p)?
        .get("captions")
        .ok_or_else(|| bad(C, "missing `captions`"))?;
    let captions = match value {
        Json::Null => None,
        Json::String(path) => {
            let path = text(C, path.clone(), "captions")?;
            if path.trim().is_empty() {
                None
            } else {
                Some(path)
            }
        }
        _ => return Err(bad(C, "`captions`: expected string or null")),
    };

    let slide = slide_mut(s, &sid)?;
    let object = find_object_mut(slide, &oid)?;
    match &mut object.kind {
        ObjectKind::Audio {
            captions: current, ..
        }
        | ObjectKind::Video {
            captions: current, ..
        } => *current = captions,
        _ => {
            return Err(bad(C, format!("object {oid:?} is not audio or video")));
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_set_media(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_media";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let volume: Option<f32> = opt(C, p, "volume")?;
    if let Some(volume) = volume
        && (!volume.is_finite() || !(0.0..=1.0).contains(&volume))
    {
        return Err(bad(C, "`volume` must be finite and within 0..=1"));
    }
    let trim = match obj(C, p)?.get("trim") {
        None => None,
        Some(Json::Null) => Some(None),
        Some(value) => {
            let trim: Trim = serde_json::from_value(value.clone())
                .map_err(|e| bad(C, format!("`trim`: {e}")))?;
            if trim.start_ms > coursecraft_course::MAX_TRIM_MS {
                return Err(bad(
                    C,
                    format!(
                        "`trim.start_ms` must be at most {}",
                        coursecraft_course::MAX_TRIM_MS
                    ),
                ));
            }
            if let Some(end_ms) = trim.end_ms {
                if end_ms > coursecraft_course::MAX_TRIM_MS {
                    return Err(bad(
                        C,
                        format!(
                            "`trim.end_ms` must be at most {}",
                            coursecraft_course::MAX_TRIM_MS
                        ),
                    ));
                }
                if end_ms <= trim.start_ms {
                    return Err(bad(C, "`trim.end_ms` must be after `trim.start_ms`"));
                }
            }
            Some(Some(trim))
        }
    };
    let mut changed = 0;
    {
        let slide = slide_mut(s, &sid)?;
        let object = find_object_mut(slide, &oid)?;
        let (current_volume, current_trim) = match &mut object.kind {
            ObjectKind::Audio { volume, trim, .. } | ObjectKind::Video { volume, trim, .. } => {
                (volume, trim)
            }
            _ => {
                return Err(EngineError::Invalid(format!(
                    "object {oid:?} is not audio or video"
                )));
            }
        };
        if let Some(volume) = volume
            && (*current_volume - volume).abs() > f32::EPSILON
        {
            *current_volume = volume;
            changed += 1;
        }
        if let Some(trim) = trim
            && *current_trim != trim
        {
            *current_trim = trim;
            changed += 1;
        }
    }
    if changed == 0 {
        s.skip_undo = true;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

fn video_add_cue(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "video.add_cue";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let slide = slide_mut(s, &sid)?;
    let duration = owner_duration(slide, &oid)
        .ok_or_else(|| EngineError::NotFound(format!("object {oid:?} is not on this slide")))?;
    let cues = video_cues_mut(slide, &oid, C)?;
    if cues.len() >= MAX_CUES {
        return Err(bad(C, format!("a video may have at most {MAX_CUES} cues")));
    }
    let at_ms = cue_at_ms(C, p, "at_ms")?.unwrap_or(0.0);
    check_cue_time(C, at_ms, duration)?;
    let name = match opt::<String>(C, p, "name")? {
        Some(name) => cue_name(C, name)?,
        None => format!("Cue {}", cues.len() + 1),
    };
    let pause: bool = opt(C, p, "pause")?.unwrap_or(false);
    let id = match opt::<String>(C, p, "id")? {
        Some(id) => {
            validate_id(C, &id)?;
            if cues.iter().any(|cue| cue.id == id) {
                return Err(bad(C, format!("cue id {id:?} is already used on {oid:?}")));
            }
            id
        }
        None => fresh_id("cue", cues.iter().map(|cue| cue.id.as_str())),
    };
    cues.push(Cue {
        id: id.clone(),
        at_ms,
        name,
        pause,
    });
    cues.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id, "cue": id }))
}

fn video_set_cue(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "video.set_cue";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let cue_id: String = req(C, p, "cue")?;
    let patch = obj(C, p)?
        .get("patch")
        .and_then(Json::as_object)
        .ok_or_else(|| bad(C, "missing `patch` object"))?;
    let slide = slide_mut(s, &sid)?;
    let duration = owner_duration(slide, &oid)
        .ok_or_else(|| EngineError::NotFound(format!("object {oid:?} is not on this slide")))?;
    let cues = video_cues_mut(slide, &oid, C)?;
    let cue = cues
        .iter_mut()
        .find(|cue| cue.id == cue_id)
        .ok_or_else(|| EngineError::NotFound(format!("cue {cue_id:?} is not on {oid:?}")))?;
    if let Some(value) = patch.get("at_ms") {
        let at_ms: f64 =
            serde_json::from_value(value.clone()).map_err(|e| bad(C, format!("`at_ms`: {e}")))?;
        check_cue_time(C, at_ms, duration)?;
        cue.at_ms = at_ms;
    }
    if let Some(value) = patch.get("name") {
        let name: String =
            serde_json::from_value(value.clone()).map_err(|e| bad(C, format!("`name`: {e}")))?;
        cue.name = cue_name(C, name)?;
    }
    if let Some(value) = patch.get("pause") {
        cue.pause =
            serde_json::from_value(value.clone()).map_err(|e| bad(C, format!("`pause`: {e}")))?;
    }
    cues.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn video_remove_cue(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "video.remove_cue";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let cue_id: String = req(C, p, "cue")?;
    let slide = slide_mut(s, &sid)?;
    {
        let cues = video_cues_mut(slide, &oid, C)?;
        let before = cues.len();
        cues.retain(|cue| cue.id != cue_id);
        if cues.len() == before {
            return Err(EngineError::NotFound(format!(
                "cue {cue_id:?} is not on {oid:?}"
            )));
        }
    }
    // A trigger naming the removed cue can never fire again: remove it in the
    // same undo step.
    let matches_cue = |trigger: &Trigger| {
        matches!(
            &trigger.when,
            Event::CueReached { object, cue } if object == &oid && cue == &cue_id
        )
    };
    slide.triggers.retain(|trigger| !matches_cue(trigger));
    for layer in &mut slide.layers {
        layer.triggers.retain(|trigger| !matches_cue(trigger));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn video_add_question_cue(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "video.add_question_cue";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    let duration = owner_duration(slide, &oid)
        .ok_or_else(|| EngineError::NotFound(format!("object {oid:?} is not on this slide")))?;
    let at_ms = cue_at_ms(C, p, "at_ms")?.unwrap_or(0.0);
    check_cue_time(C, at_ms, duration)?;
    let layer_id = fresh_id("cue_layer", slide.local_ids());
    let prompt_id = fresh_id("cue_prompt", slide.local_ids());
    let continue_id = fresh_id("cue_continue", slide.local_ids());
    let name = match opt::<String>(C, p, "name")? {
        Some(name) => cue_name(C, name)?,
        None => {
            let n = match &slide.object(&oid).map(|(obj, _)| &obj.kind) {
                Some(ObjectKind::Video { cues, .. }) => {
                    cues.iter().filter(|cue| cue.pause).count() + 1
                }
                _ => 1,
            };
            format!("Question {n}")
        }
    };
    let cues = video_cues_mut(slide, &oid, C)?;
    if cues.len() >= MAX_CUES {
        return Err(bad(C, format!("a video may have at most {MAX_CUES} cues")));
    }
    let cue_id = fresh_id("cue", cues.iter().map(|cue| cue.id.as_str()));
    cues.push(Cue {
        id: cue_id.clone(),
        at_ms,
        name: name.clone(),
        pause: true,
    });
    cues.sort_by(|a, b| a.at_ms.total_cmp(&b.at_ms));

    let w = size.w as f32;
    let h = size.h as f32;
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(bad(C, "slide size is invalid"));
    }
    let margin = (w * 0.08).clamp(24.0, 160.0);
    let panel_w = (w - 2.0 * margin).max(80.0);
    let top = (h * 0.12).max(24.0);
    let button_h = (h * 0.08).clamp(36.0, 64.0);
    let gap = (h * 0.025).clamp(8.0, 24.0);
    let prompt_h = (h * 0.16).clamp(56.0, 140.0);

    let mut objects = Vec::new();
    let prompt = interaction_text(
        prompt_id.clone(),
        format!("{name} prompt"),
        "Question prompt".to_owned(),
        Rect {
            x: margin,
            y: top,
            w: panel_w,
            h: prompt_h,
        },
        &theme,
    );
    objects.push(prompt);
    let mut choice_ids = Vec::new();
    let choice_top = top + prompt_h + gap;
    for i in 0..3 {
        let id = fresh_id(
            &format!("cue_choice{}", i + 1),
            slide
                .local_ids()
                .chain(objects.iter().map(|object: &Object| object.id.as_str())),
        );
        choice_ids.push(id.clone());
        objects.push(interaction_button(
            id,
            format!("{name} choice {}", i + 1),
            format!("Choice {}", i + 1),
            Rect {
                x: margin,
                y: choice_top + (button_h + gap) * i as f32,
                w: panel_w,
                h: button_h,
            },
            i == 0,
            &theme,
        ));
    }
    let continue_y = (choice_top + (button_h + gap) * 3.0)
        .min(h - margin - button_h)
        .max(0.0);
    objects.push(interaction_button(
        continue_id.clone(),
        format!("{name} Continue"),
        "Continue".to_owned(),
        Rect {
            x: (w - margin - (w * 0.2).clamp(120.0, 200.0)).max(0.0),
            y: continue_y,
            w: (w * 0.2).clamp(120.0, 200.0),
            h: button_h,
        },
        false,
        &theme,
    ));
    let layer = Layer {
        id: layer_id.clone(),
        name: name.clone(),
        duration_ms: 5000,
        objects,
        triggers: Vec::new(),
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: true,
        // The video element draws above the canvas, so it must hide or it covers the question.
        hide_base_objects: true,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
    };
    slide.layers.push(layer);
    slide.triggers.push(interaction_trigger(
        Action::ShowLayer {
            layer: layer_id.clone(),
        },
        Event::CueReached {
            object: oid.clone(),
            cue: cue_id.clone(),
        },
    ));
    slide.triggers.push(interaction_trigger(
        Action::HideLayer {
            layer: Some(layer_id.clone()),
        },
        Event::Click {
            object: continue_id.clone(),
        },
    ));
    slide.triggers.push(interaction_trigger(
        Action::PlayMedia {
            object: oid.clone(),
        },
        Event::Click {
            object: continue_id.clone(),
        },
    ));
    keep_valid(s, was_valid)?;
    Ok(json!({
        "id": cue_id,
        "cue": cue_id,
        "layer": layer_id,
        "objects": choice_ids,
        "prompt": prompt_id,
        "continue": continue_id,
    }))
}

fn owner_duration(slide: &Slide, object: &str) -> Option<u32> {
    match slide.object(object) {
        Some((_, None)) => Some(slide.duration_ms),
        Some((_, Some(layer))) => slide.layer(layer).map(|layer| layer.duration_ms),
        None => None,
    }
}

fn video_cues_mut<'a>(slide: &'a mut Slide, object: &str, cmd: &str) -> Result<&'a mut Vec<Cue>> {
    let obj = find_object_mut(slide, object)?;
    match &mut obj.kind {
        ObjectKind::Video { cues, .. } => Ok(cues),
        _ => Err(bad(cmd, format!("object {object:?} is not a video"))),
    }
}

fn cue_at_ms(cmd: &str, p: &Json, key: &str) -> Result<Option<f64>> {
    let Some(value) = opt::<f64>(cmd, p, key)? else {
        return Ok(None);
    };
    if !value.is_finite() || value < 0.0 {
        return Err(bad(cmd, format!("`{key}` must be finite and at least 0")));
    }
    Ok(Some(value))
}

fn check_cue_time(cmd: &str, at_ms: f64, duration: u32) -> Result<()> {
    if !at_ms.is_finite() || at_ms < 0.0 {
        return Err(bad(cmd, "cue time must be finite and at least 0"));
    }
    if at_ms > f64::from(duration) {
        return Err(bad(
            cmd,
            format!("cue time {at_ms} ms is past the video's known duration ({duration} ms)"),
        ));
    }
    Ok(())
}

fn cue_name(cmd: &str, name: String) -> Result<String> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(bad(cmd, "cue name cannot be empty"));
    }
    text(cmd, name, "name")
}

fn object_add_state(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let sid: String = req("object.add_state", p, "slide")?;
    let oid: String = req("object.add_state", p, "object")?;
    let state: StateDef = req("object.add_state", p, "state")?;
    find_object_mut(slide_mut(s, &sid)?, &oid)?
        .states
        .push(state);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_update_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update_state";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let state: StateDef = req(C, p, "state")?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    let target = obj
        .states
        .iter_mut()
        .find(|s| s.name == state.name)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "state {:?} not found on object {oid:?}",
                state.name
            ))
        })?;
    *target = state;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_delete_state(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.delete_state";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let name: String = req(C, p, "name")?;

    let slide = slide_mut(s, &sid)?;
    let obj = find_object_mut(slide, &oid)?;
    let before = obj.states.len();
    obj.states.retain(|s| s.name != name);
    if obj.states.len() == before {
        return Err(EngineError::NotFound(format!(
            "state {name:?} not found on object {oid:?}"
        )));
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let sid: String = req("object.delete", p, "slide")?;
    let oid: String = req("object.delete", p, "object")?;
    let slide = slide_mut(s, &sid)?;
    let is_panel = slide
        .object(&oid)
        .is_some_and(|(object, _)| matches!(object.kind, ObjectKind::ScrollPanel { .. }));
    let before = slide.local_ids().count();
    slide.objects.retain(|o| o.id != oid);
    for l in &mut slide.layers {
        l.objects.retain(|o| o.id != oid);
    }
    if slide.local_ids().count() == before {
        return Err(EngineError::NotFound(format!(
            "object {oid:?} is not on slide {sid:?}"
        )));
    }
    // Deleting a panel releases every child in the same undo step.
    if is_panel {
        for object in &mut slide.objects {
            if object.panel.as_deref() == Some(oid.as_str()) {
                object.panel = None;
            }
        }
        for layer in &mut slide.layers {
            for object in &mut layer.objects {
                if object.panel.as_deref() == Some(oid.as_str()) {
                    object.panel = None;
                }
            }
        }
    }
    // A trigger that still points at the object makes the course invalid, so
    // keep_valid refuses the delete and names the trigger.
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// Default shape text style: a contrasting colour on the shape's fill, centred
/// both ways. Transparent fills read as the slide background, so use dark text.
fn shape_text_default_style(fill: &Color) -> TextStyle {
    let dark = Color("#1F1F1F".to_owned());
    let light = Color("#FFFFFF".to_owned());
    let color = match parse_color(fill) {
        Some(fill) if fill.a > 0 => {
            let luminance = relative_luminance(fill);
            if luminance < 0.45 { light } else { dark }
        }
        _ => dark,
    };
    TextStyle {
        color,
        align: Align::Center,
        valign: VAlign::Middle,
        ..TextStyle::default()
    }
}

/// Mutable parts of an object that carries text. A shape with no text yet gets
/// a default style when a text command touches it.
struct TextParts<'a> {
    text: &'a mut String,
    runs: &'a mut Vec<TextRun>,
    style: &'a mut TextStyle,
    paragraphs: &'a mut Vec<Paragraph>,
    autofit: Option<&'a mut Autofit>,
    /// A table cell keeps vertical alignment beside its style.
    valign: Option<&'a mut VAlign>,
}

fn text_parts(kind: &mut ObjectKind) -> Option<TextParts<'_>> {
    match kind {
        ObjectKind::Text {
            text,
            runs,
            style,
            paragraphs,
            autofit,
            ..
        } => Some(TextParts {
            text,
            runs,
            style,
            paragraphs,
            autofit: Some(autofit),
            valign: None,
        }),
        ObjectKind::Shape { fill, text, .. } => {
            if text.is_none() {
                *text = Some(ShapeText {
                    text: String::new(),
                    style: shape_text_default_style(fill),
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: Autofit::None,
                });
            }
            let shape = text.as_mut()?;
            Some(TextParts {
                text: &mut shape.text,
                runs: &mut shape.runs,
                style: &mut shape.style,
                paragraphs: &mut shape.paragraphs,
                autofit: Some(&mut shape.autofit),
                valign: None,
            })
        }
        _ => None,
    }
}

/// Text parts of an object, or of the table cell named by `row` and `column`.
fn text_parts_at<'a>(
    kind: &'a mut ObjectKind,
    p: &Json,
    cmd: &str,
) -> Result<Option<TextParts<'a>>> {
    if let ObjectKind::Table { rows, .. } = kind {
        let row: usize =
            opt(cmd, p, "row")?.ok_or_else(|| bad(cmd, "a table cell needs `row` and `column`"))?;
        let column: usize = opt(cmd, p, "column")?
            .ok_or_else(|| bad(cmd, "a table cell needs `row` and `column`"))?;
        let cell = rows
            .get_mut(row)
            .and_then(|line| line.cells.get_mut(column))
            .ok_or_else(|| EngineError::NotFound(format!("no table cell {row},{column}")))?;
        return Ok(Some(TextParts {
            text: &mut cell.text,
            runs: &mut cell.runs,
            style: &mut cell.style,
            paragraphs: &mut cell.paragraphs,
            autofit: None,
            valign: Some(&mut cell.valign),
        }));
    }
    Ok(text_parts(kind))
}

fn text_set(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let new_text_raw: String = req(C, p, "text")?;
    let new_text = text(C, new_text_raw, "text")?;

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    // Diff against what is displayed: the runs when there are any.
    let old_plain: String = if parts.runs.is_empty() {
        parts.text.clone()
    } else {
        parts.runs.iter().map(|r| r.text.as_str()).collect()
    };
    let new_runs = apply_text_edit(parts.runs, &old_plain, &new_text);
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    let new_paragraphs = align_paragraphs(&old_plain, parts.paragraphs, &new_text);
    *parts.runs = new_runs;
    *parts.paragraphs = new_paragraphs;
    *parts.text = new_text;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn text_style_range(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.style_range";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: usize = req(C, p, "start")?;
    let end: usize = req(C, p, "end")?;
    let bold: Option<bool> = opt(C, p, "bold")?;
    let italic: Option<bool> = opt(C, p, "italic")?;
    let underline: Option<bool> = opt(C, p, "underline")?;
    let strikethrough: Option<bool> = opt(C, p, "strikethrough")?;
    let baseline: Option<coursecraft_course::Baseline> = opt(C, p, "baseline")?;
    let letter_spacing: Option<f32> = opt(C, p, "letter_spacing")?;
    if letter_spacing.is_some_and(|v| !v.is_finite()) {
        return Err(bad(C, "letter_spacing must be finite"));
    }
    let size: Option<f32> = opt(C, p, "size")?;
    let color_str: Option<String> = opt(C, p, "color")?;
    let font: Option<String> = opt(C, p, "font")?;

    if let Some(sz) = size
        && (!sz.is_finite() || sz <= 0.0)
    {
        return Err(bad(C, "size must be a finite positive number"));
    }
    let color = if let Some(cs) = color_str {
        let col = Color(cs);
        validate_color(C, &col)?;
        Some(col)
    } else {
        None
    };

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let total_chars = parts.text.chars().count();
    let (range_start, range_end) = if start == 0 && end == 0 {
        (0, 0)
    } else {
        let s_clamp = start.min(total_chars);
        let e_clamp = end.min(total_chars);
        (s_clamp.min(e_clamp), s_clamp.max(e_clamp))
    };

    let patch = TextRunPatch {
        bold,
        italic,
        underline,
        size,
        color,
        font,
        link: None,
        strikethrough,
        baseline,
        letter_spacing,
    };
    let new_runs = apply_style_to_range(parts.runs, parts.text, (range_start, range_end), &patch);
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    *parts.runs = new_runs;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn text_set_paragraph(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_paragraph";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let align_str: Option<String> = opt(C, p, "align")?;
    let valign_str: Option<String> = opt(C, p, "valign")?;

    let align = match align_str.as_deref() {
        None => None,
        Some("left") => Some(Align::Left),
        Some("center") => Some(Align::Center),
        Some("right") => Some(Align::Right),
        Some(other) => return Err(bad(C, format!("unknown align {other:?}"))),
    };
    let valign = match valign_str.as_deref() {
        None => None,
        Some("top") => Some(VAlign::Top),
        Some("middle") => Some(VAlign::Middle),
        Some("bottom") => Some(VAlign::Bottom),
        Some(other) => return Err(bad(C, format!("unknown valign {other:?}"))),
    };

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(mut parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    if let Some(a) = align {
        parts.style.align = a;
    }
    if let Some(v) = valign {
        if let Some(slot) = parts.valign.as_deref_mut() {
            *slot = v;
        } else {
            parts.style.valign = v;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn trim_paragraphs(paragraphs: &mut Vec<Paragraph>) {
    while paragraphs.last() == Some(&Paragraph::default()) {
        paragraphs.pop();
    }
}

fn text_set_paragraphs(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_paragraphs";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: Option<usize> = opt(C, p, "start")?;
    let end: Option<usize> = opt(C, p, "end")?;
    let list_str: Option<String> = opt(C, p, "list")?;
    let level: Option<u8> = opt(C, p, "level")?;
    let line_spacing: Option<f32> = opt(C, p, "line_spacing")?;
    let space_before: Option<f32> = opt(C, p, "space_before")?;
    let space_after: Option<f32> = opt(C, p, "space_after")?;

    let list = match list_str.as_deref() {
        None => None,
        Some("none") => Some(ListKind::None),
        Some("bullet") => Some(ListKind::Bullet),
        Some("number") => Some(ListKind::Number),
        Some(other) => return Err(bad(C, format!("unknown list {other:?}"))),
    };
    if let Some(level) = level
        && level > 4
    {
        return Err(bad(C, "level must be within 0..=4"));
    }
    if let Some(spacing) = line_spacing
        && (!spacing.is_finite() || !(0.8..=3.0).contains(&spacing))
    {
        return Err(bad(C, "line_spacing must be finite and within 0.8..=3"));
    }
    for (name, value) in [("space_before", space_before), ("space_after", space_after)] {
        if let Some(value) = value
            && (!value.is_finite() || !(0.0..=200.0).contains(&value))
        {
            return Err(bad(C, format!("{name} must be finite and within 0..=200")));
        }
    }

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let count = coursecraft_course::text::paragraph_count(parts.text);
    let first = start.unwrap_or(0).min(count);
    let last = end.unwrap_or(count).clamp(first, count);
    let mut paragraphs = paragraphs_for(parts.text, parts.paragraphs);
    for paragraph in paragraphs.iter_mut().skip(first).take(last - first) {
        if let Some(list) = list {
            paragraph.list = list;
        }
        if let Some(level) = level {
            paragraph.level = level;
        }
        if let Some(spacing) = line_spacing {
            paragraph.line_spacing = spacing;
        }
        if let Some(value) = space_before {
            paragraph.space_before = value;
        }
        if let Some(value) = space_after {
            paragraph.space_after = value;
        }
    }
    trim_paragraphs(&mut paragraphs);
    *parts.paragraphs = paragraphs;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn text_set_link(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text.set_link";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let start: Option<usize> = opt(C, p, "start")?;
    let end: Option<usize> = opt(C, p, "end")?;
    let link: Option<Link> = opt(C, p, "link")?;

    if let Some(link) = &link {
        match link {
            Link::Url { url } if !is_valid_link_url(url) => {
                return Err(bad(
                    C,
                    format!("link {url:?} must start with http://, https:// or mailto:"),
                ));
            }
            Link::Slide { slide } if s.course.play_slide(slide).is_none() => {
                return Err(bad(
                    C,
                    format!("link target slide {slide:?} does not exist"),
                ));
            }
            _ => {}
        }
    }

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let total_chars = parts.text.chars().count();
    let start = start.unwrap_or(0).min(total_chars);
    let end = end.unwrap_or(total_chars).min(total_chars);
    let (start, end) = if start == 0 && end == total_chars {
        (0, 0)
    } else {
        (start.min(end), start.max(end))
    };
    let new_runs = apply_link_to_range(parts.runs, parts.text, (start, end), link);
    if new_runs.len() > 1000 {
        return Err(bad(C, "exceeded 1000 text runs"));
    }
    *parts.runs = new_runs;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_set_autofit(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_autofit";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let autofit: Autofit = req(C, p, "autofit")?;

    let sl = slide_mut(s, &sid)?;
    let obj = find_object_mut(sl, &oid)?;
    let Some(parts) = text_parts_at(&mut obj.kind, p, C)? else {
        return Err(bad(C, format!("object {oid:?} is not a text object")));
    };
    let Some(current) = parts.autofit else {
        return Err(bad(C, "a table cell has no autofit setting"));
    };
    *current = autofit;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

type TableRefs<'a> = (
    &'a mut Vec<f32>,
    &'a mut Vec<TableRow>,
    &'a mut TableBorders,
    &'a mut bool,
);

fn table_mut<'a>(s: &'a mut Session, sid: &str, oid: &str, cmd: &str) -> Result<TableRefs<'a>> {
    let slide = slide_mut(s, sid)?;
    let object = find_object_mut(slide, oid)?;
    match &mut object.kind {
        ObjectKind::Table {
            columns,
            rows,
            borders,
            header_row,
        } => Ok((columns, rows, borders, header_row)),
        _ => Err(bad(cmd, format!("object {oid:?} is not a table"))),
    }
}

fn normalize_row_heights(rows: &mut [TableRow]) {
    let mut heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    normalize_table_fractions(&mut heights);
    for (row, height) in rows.iter_mut().zip(heights) {
        row.height = height;
    }
}

fn table_set_cell(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_cell";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let new_text: Option<String> = opt(C, p, "text")?;
    let runs: Option<Vec<TextRun>> = opt(C, p, "runs")?;
    let paragraphs: Option<Vec<Paragraph>> = opt(C, p, "paragraphs")?;
    let valign: Option<VAlign> = opt(C, p, "valign")?;

    let (_, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let cell = rows
        .get_mut(row)
        .and_then(|line| line.cells.get_mut(column))
        .ok_or_else(|| EngineError::NotFound(format!("no table cell {row},{column}")))?;

    let text_was_set = new_text.is_some();
    if let Some(value) = new_text {
        let value = text(C, value, "text")?;
        let old_plain = if cell.runs.is_empty() {
            cell.text.clone()
        } else {
            cell.runs.iter().map(|run| run.text.as_str()).collect()
        };
        if runs.is_none() {
            cell.runs = apply_text_edit(&cell.runs, &old_plain, &value);
            cell.paragraphs = align_paragraphs(&old_plain, &cell.paragraphs, &value);
        }
        cell.text = value;
    }
    if let Some(runs) = runs {
        if runs.len() > 1000 {
            return Err(bad(C, "exceeded 1000 text runs"));
        }
        let joined: String = runs.iter().map(|run| run.text.as_str()).collect();
        if !text_was_set {
            cell.text = joined;
        }
        cell.runs = runs;
    }
    if let Some(paragraphs) = paragraphs {
        cell.paragraphs = paragraphs;
    }
    if let Some(valign) = valign {
        cell.valign = valign;
    }
    if let Some(value) = obj(C, p)?.get("fill") {
        cell.fill = match value {
            Json::Null => None,
            _ => {
                let fill: Color = serde_json::from_value(value.clone())
                    .map_err(|e| bad(C, format!("`fill`: {e}")))?;
                validate_color(C, &fill)?;
                Some(fill)
            }
        };
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_insert_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.insert_row";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if rows.len() >= coursecraft_course::TABLE_MAX_AXIS {
        return Err(bad(C, "a table may have at most 50 rows"));
    }
    let at = index.unwrap_or(rows.len());
    if at > rows.len() {
        return Err(bad(C, "row index is past the end of the table"));
    }
    // Extend spans that cross the insertion line before adding the row.
    for (row_index, row) in rows.iter_mut().enumerate() {
        for cell in &mut row.cells {
            let span = usize::from(cell.row_span.max(1));
            if row_index < at && at < row_index.saturating_add(span) {
                cell.row_span = u8::try_from(span.saturating_add(1)).unwrap_or(u8::MAX);
            }
        }
    }
    let mut heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    normalize_table_fractions(&mut heights);
    let old_count = heights.len() as f32;
    let new_height = 1.0 / (old_count + 1.0);
    for (row, height) in rows.iter_mut().zip(heights) {
        row.height = height * old_count / (old_count + 1.0);
    }
    let cells = vec![TableCell::default(); columns.len()];
    rows.insert(
        at.min(rows.len()),
        TableRow {
            height: new_height,
            cells,
        },
    );
    normalize_row_heights(rows);
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": at }))
}

fn table_delete_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.delete_row";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let (_, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if rows.len() <= 1 {
        return Err(bad(C, "a table needs at least one row"));
    }
    if index >= rows.len() {
        return Err(EngineError::NotFound(format!("no table row {index}")));
    }
    for (row_index, row) in rows.iter_mut().enumerate() {
        for cell in &mut row.cells {
            let span = usize::from(cell.row_span.max(1));
            if row_index < index && index < row_index.saturating_add(span) {
                cell.row_span = u8::try_from(span.saturating_sub(1)).unwrap_or(1).max(1);
            }
        }
    }
    rows.remove(index);
    let max_span = rows.len().max(1);
    for row in rows.iter_mut() {
        for cell in &mut row.cells {
            cell.row_span = cell
                .row_span
                .min(u8::try_from(max_span).unwrap_or(u8::MAX))
                .max(1);
        }
    }
    normalize_row_heights(rows);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_insert_column(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.insert_column";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if columns.len() >= coursecraft_course::TABLE_MAX_AXIS {
        return Err(bad(C, "a table may have at most 50 columns"));
    }
    let at = index.unwrap_or(columns.len());
    if at > columns.len() {
        return Err(bad(C, "column index is past the end of the table"));
    }
    for row in rows.iter_mut() {
        for (column, cell) in row.cells.iter_mut().enumerate() {
            let span = usize::from(cell.col_span.max(1));
            if column < at && at < column.saturating_add(span) {
                cell.col_span = u8::try_from(span.saturating_add(1)).unwrap_or(u8::MAX);
            }
        }
        row.cells
            .insert(at.min(row.cells.len()), TableCell::default());
    }
    let mut widths = std::mem::take(columns);
    normalize_table_fractions(&mut widths);
    let old_count = widths.len() as f32;
    let new_width = 1.0 / (old_count + 1.0);
    for width in &mut widths {
        *width *= old_count / (old_count + 1.0);
    }
    let at = at.min(widths.len());
    widths.insert(at, new_width);
    normalize_table_fractions(&mut widths);
    *columns = widths;
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": at }))
}

fn table_delete_column(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.delete_column";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let index: usize = req(C, p, "index")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if columns.len() <= 1 {
        return Err(bad(C, "a table needs at least one column"));
    }
    if index >= columns.len() {
        return Err(EngineError::NotFound(format!("no table column {index}")));
    }
    for row in rows.iter_mut() {
        for (column, cell) in row.cells.iter_mut().enumerate() {
            let span = usize::from(cell.col_span.max(1));
            if column < index && index < column.saturating_add(span) {
                cell.col_span = u8::try_from(span.saturating_sub(1)).unwrap_or(1).max(1);
            }
        }
        if index < row.cells.len() {
            row.cells.remove(index);
        }
        let max_span = row.cells.len().max(1);
        for cell in &mut row.cells {
            cell.col_span = cell
                .col_span
                .min(u8::try_from(max_span).unwrap_or(u8::MAX))
                .max(1);
        }
    }
    columns.remove(index);
    normalize_table_fractions(columns);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_resize(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.resize";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let new_columns: Option<Vec<f32>> = opt(C, p, "columns")?;
    let new_rows: Option<Vec<f32>> = opt(C, p, "rows")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    if let Some(mut values) = new_columns {
        if values.len() != columns.len() || values.is_empty() {
            return Err(bad(C, "columns must have one fraction per column"));
        }
        if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
            return Err(bad(C, "column fractions must be finite and above 0"));
        }
        normalize_table_fractions(&mut values);
        *columns = values;
    }
    if let Some(mut values) = new_rows {
        if values.len() != rows.len() || values.is_empty() {
            return Err(bad(C, "rows must have one fraction per row"));
        }
        if !values.iter().all(|value| value.is_finite() && *value > 0.0) {
            return Err(bad(C, "row fractions must be finite and above 0"));
        }
        normalize_table_fractions(&mut values);
        for (row, height) in rows.iter_mut().zip(values) {
            row.height = height;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_merge(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.merge";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let row_count: usize = req(C, p, "rows")?;
    let column_count: usize = req(C, p, "columns")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let row_end = row.saturating_add(row_count);
    let column_end = column.saturating_add(column_count);
    if row_count == 0 || column_count == 0 || row_end > rows.len() || column_end > columns.len() {
        return Err(bad(C, "the merged range must lie inside the table"));
    }
    let owners = table_cell_owners(rows, columns.len());
    if owners
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
        != Some((row, column))
    {
        return Err(bad(C, "the merge anchor is covered by another merge"));
    }
    for covered_row in row..row_end {
        for covered_column in column..column_end {
            let Some(cell) = rows
                .get_mut(covered_row)
                .and_then(|line| line.cells.get_mut(covered_column))
            else {
                continue;
            };
            if (covered_row, covered_column) != (row, column) {
                *cell = TableCell::default();
            }
        }
    }
    let Some(anchor) = rows
        .get_mut(row)
        .and_then(|line| line.cells.get_mut(column))
    else {
        return Err(bad(C, "the merge anchor is missing"));
    };
    anchor.row_span = u8::try_from(row_count).unwrap_or(u8::MAX).max(1);
    anchor.col_span = u8::try_from(column_count).unwrap_or(u8::MAX).max(1);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_split(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.split";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let row: usize = req(C, p, "row")?;
    let column: usize = req(C, p, "column")?;
    let (columns, rows, _, _) = table_mut(s, &sid, &oid, C)?;
    let owners = table_cell_owners(rows, columns.len());
    let Some(owner) = owners
        .get(row)
        .and_then(|line| line.get(column))
        .copied()
        .flatten()
    else {
        return Err(EngineError::NotFound(format!(
            "no table cell {row},{column}"
        )));
    };
    let Some(cell) = rows
        .get_mut(owner.0)
        .and_then(|line| line.cells.get_mut(owner.1))
    else {
        return Err(EngineError::NotFound(format!(
            "no table cell {row},{column}"
        )));
    };
    cell.row_span = 1;
    cell.col_span = 1;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_set_borders(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_borders";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let has_outer = obj(C, p)?.contains_key("outer");
    let has_inner = obj(C, p)?.contains_key("inner");
    let outer: Option<coursecraft_course::Outline> =
        if has_outer { opt(C, p, "outer")? } else { None };
    let inner: Option<coursecraft_course::Outline> =
        if has_inner { opt(C, p, "inner")? } else { None };
    let (_, _, borders, _) = table_mut(s, &sid, &oid, C)?;
    if has_outer {
        borders.outer = outer;
    }
    if has_inner {
        borders.inner = inner;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn table_set_header_row(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "table.set_header_row";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let header_row: bool = req(C, p, "header_row")?;
    let (_, _, _, current) = table_mut(s, &sid, &oid, C)?;
    *current = header_row;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn triggers_mut<'a>(slide: &'a mut Slide, layer: Option<&str>) -> Result<&'a mut Vec<Trigger>> {
    match layer {
        Some(l) => Ok(&mut layer_mut(slide, l)?.triggers),
        None => Ok(&mut slide.triggers),
    }
}

fn trigger_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "trigger.add";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let trigger: Trigger = req(C, p, "trigger")?;
    let index: Option<usize> = opt(C, p, "index")?;
    let list = triggers_mut(slide_mut(s, &sid)?, layer.as_deref())?;
    let at = index.unwrap_or(list.len()).min(list.len());
    list.insert(at, trigger);
    keep_valid(s, was_valid)?;
    Ok(json!({ "index": at }))
}

fn trigger_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "trigger.update";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let index: usize = req(C, p, "index")?;
    let trigger: Trigger = req(C, p, "trigger")?;

    let list = triggers_mut(slide_mut(s, &sid)?, layer.as_deref())?;
    if index >= list.len() {
        return Err(EngineError::NotFound(format!(
            "no trigger at index {index}"
        )));
    }
    let target = list
        .get_mut(index)
        .ok_or_else(|| EngineError::NotFound(format!("no trigger at index {index}")))?;
    *target = trigger;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn trigger_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "trigger.move";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let layer: Option<String> = opt(C, p, "layer")?;
    let from: usize = req(C, p, "from")?;
    let to: usize = req(C, p, "to")?;

    let list = triggers_mut(slide_mut(s, &sid)?, layer.as_deref())?;
    if from >= list.len() {
        return Err(EngineError::NotFound(format!("no trigger at index {from}")));
    }
    let t = list.remove(from);
    let at = to.min(list.len());
    list.insert(at, t);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn trigger_delete(s: &mut Session, p: &Json) -> Result<Json> {
    let sid: String = req("trigger.delete", p, "slide")?;
    let layer: Option<String> = opt("trigger.delete", p, "layer")?;
    let index: usize = req("trigger.delete", p, "index")?;
    let list = triggers_mut(slide_mut(s, &sid)?, layer.as_deref())?;
    if index >= list.len() {
        return Err(EngineError::NotFound(format!(
            "no trigger at index {index}"
        )));
    }
    list.remove(index);
    Ok(Json::Null)
}

// ---------------------------------------------------------------- resources and glossary

fn resource_index(s: &Session, cmd: &str, id: &str) -> Result<usize> {
    let _ = cmd;
    s.course
        .resources
        .iter()
        .position(|r| r.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("resource {id:?} does not exist")))
}

fn glossary_index(s: &Session, id: &str) -> Result<usize> {
    s.course
        .glossary
        .iter()
        .position(|g| g.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("glossary entry {id:?} does not exist")))
}

fn resource_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.add";
    let was_valid = s.course.is_playable();
    let title = text(C, req(C, p, "title")?, "title")?;
    let target: coursecraft_course::ResourceTarget = req(C, p, "target")?;
    let id = fresh_id("resource", s.course.resources.iter().map(|r| r.id.as_str()));
    s.course.resources.push(coursecraft_course::Resource {
        id: id.clone(),
        title,
        target,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn resource_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.update";
    let was_valid = s.course.is_playable();
    let id: String = req(C, p, "id")?;
    let title: Option<String> = opt(C, p, "title")?;
    let target: Option<coursecraft_course::ResourceTarget> = opt(C, p, "target")?;
    let index = resource_index(s, C, &id)?;
    if let Some(resource) = s.course.resources.get_mut(index) {
        if let Some(title) = title {
            resource.title = text(C, title, "title")?;
        }
        if let Some(target) = target {
            resource.target = target;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn resource_remove(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.remove";
    let id: String = req(C, p, "id")?;
    let index = resource_index(s, C, &id)?;
    s.course.resources.remove(index);
    Ok(Json::Null)
}

fn resource_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "resource.move";
    let id: String = req(C, p, "id")?;
    let to: usize = req(C, p, "to")?;
    let index = resource_index(s, C, &id)?;
    if to >= s.course.resources.len() {
        return Err(bad(C, "`to` is past the end of the list"));
    }
    move_to(&mut s.course.resources, index, to);
    Ok(Json::Null)
}

fn glossary_add(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.add";
    let was_valid = s.course.is_playable();
    let term = text(C, req(C, p, "term")?, "term")?;
    let definition: Option<String> = opt(C, p, "definition")?;
    let definition = text(C, definition.unwrap_or_default(), "definition")?;
    let id = fresh_id("term", s.course.glossary.iter().map(|g| g.id.as_str()));
    s.course.glossary.push(coursecraft_course::GlossaryEntry {
        id: id.clone(),
        term,
        definition,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn glossary_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.update";
    let was_valid = s.course.is_playable();
    let id: String = req(C, p, "id")?;
    let term: Option<String> = opt(C, p, "term")?;
    let definition: Option<String> = opt(C, p, "definition")?;
    let index = glossary_index(s, &id)?;
    if let Some(entry) = s.course.glossary.get_mut(index) {
        if let Some(term) = term {
            entry.term = text(C, term, "term")?;
        }
        if let Some(definition) = definition {
            entry.definition = text(C, definition, "definition")?;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn glossary_remove(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.remove";
    let id: String = req(C, p, "id")?;
    let index = glossary_index(s, &id)?;
    s.course.glossary.remove(index);
    Ok(Json::Null)
}

fn glossary_move(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "glossary.move";
    let id: String = req(C, p, "id")?;
    let to: usize = req(C, p, "to")?;
    let index = glossary_index(s, &id)?;
    if to >= s.course.glossary.len() {
        return Err(bad(C, "`to` is past the end of the list"));
    }
    move_to(&mut s.course.glossary, index, to);
    Ok(Json::Null)
}

fn variable_add(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let name: String = req("variable.add", p, "name")?;
    let value: Value = req("variable.add", p, "value")?;
    if s.course.variable(&name).is_some() {
        return Err(EngineError::Invalid(format!(
            "variable {name:?} already exists"
        )));
    }
    s.course.variables.push(VariableDef { name, value });
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn variable_set_value(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "variable.set_value";
    let was_valid = s.course.is_playable();
    let name: String = req(C, p, "name")?;
    let value: Value = req(C, p, "value")?;

    if let Value::Number(n) = &value
        && !n.is_finite()
    {
        return Err(bad(C, "variable value must be finite"));
    }

    let var = s
        .course
        .variables
        .iter_mut()
        .find(|v| v.name == name)
        .ok_or_else(|| EngineError::NotFound(format!("variable {name:?} does not exist")))?;

    if var.value.kind() != value.kind() {
        return Err(EngineError::Invalid(format!(
            "cannot change variable type from {:?} to {:?}",
            var.value.kind(),
            value.kind()
        )));
    }
    var.value = value;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn condition_references_var(c: &Condition, var_name: &str) -> bool {
    match c {
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(|c| condition_references_var(c, var_name))
        }
        Condition::Not(inner) => condition_references_var(inner, var_name),
        Condition::Compare { variable, .. } => variable == var_name,
        Condition::ObjectState { .. } => false,
    }
}

fn trigger_references_var(t: &Trigger, var_name: &str) -> bool {
    let in_event = match &t.when {
        Event::VariableChanges { variable } => variable == var_name,
        _ => false,
    };
    let in_action = match &t.action {
        Action::SetVariable { variable, .. } => variable == var_name,
        _ => false,
    };
    let in_condition = t
        .condition
        .as_ref()
        .is_some_and(|c| condition_references_var(c, var_name));
    in_event || in_action || in_condition
}

fn variable_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "variable.delete";
    let was_valid = s.course.is_playable();
    let name: String = req(C, p, "name")?;

    let pos = s
        .course
        .variables
        .iter()
        .position(|v| v.name == name)
        .ok_or_else(|| EngineError::NotFound(format!("variable {name:?} does not exist")))?;

    for sc in &s.course.scenes {
        for sl in &sc.slides {
            for (i, t) in sl.triggers.iter().enumerate() {
                if trigger_references_var(t, &name) {
                    return Err(EngineError::Invalid(format!(
                        "variable {name:?} is referenced by trigger {i} on slide {:?}",
                        sl.id
                    )));
                }
            }
            for l in &sl.layers {
                for (i, t) in l.triggers.iter().enumerate() {
                    if trigger_references_var(t, &name) {
                        return Err(EngineError::Invalid(format!(
                            "variable {name:?} is referenced by trigger {i} on layer {:?} of slide {:?}",
                            l.id, sl.id
                        )));
                    }
                }
            }
        }
    }

    s.course.variables.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// The default question `question.insert` starts from, matching the editor's old
/// `insert_question` defaults.
fn default_multiple_choice_question() -> Question {
    Question {
        kind: QuestionKind::MultipleChoice,
        prompt: "Enter question prompt here".to_owned(),
        choices: vec![
            Choice {
                id: "a".to_owned(),
                text: "Choice 1".to_owned(),
                correct: true,
                points: 0,
            },
            Choice {
                id: "b".to_owned(),
                text: "Choice 2".to_owned(),
                correct: false,
                points: 0,
            },
        ],
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
    }
}

/// Add the placeholder objects a hotspot or drag-and-drop question needs so the
/// course stays valid before the author picks real content in the panel.
fn ensure_question_support(slide: &mut Slide, kind: QuestionKind, theme: &Theme) {
    match kind {
        QuestionKind::Hotspot => {
            let has_image = slide
                .objects
                .iter()
                .any(|object| matches!(object.kind, ObjectKind::Image { .. }));
            if !has_image {
                let id = fresh_id("image", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Hotspot image".to_owned(),
                    kind: ObjectKind::Image {
                        src: "placeholder.png".to_owned(),
                        alt: "Choose an image for the hotspot".to_owned(),
                        crop: None,
                    },
                    frame: Rect {
                        x: 240.0,
                        y: 180.0,
                        w: 800.0,
                        h: 360.0,
                    },
                    timing: Timing::default(),
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
        }
        QuestionKind::DragDrop => {
            let has_named =
                |slide: &Slide, name: &str| slide.objects.iter().any(|object| object.name == name);
            if !has_named(slide, "Drag me") {
                let id = fresh_id("button", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Drag me".to_owned(),
                    kind: ObjectKind::Button {
                        label: "Drag me".to_owned(),
                        fill: theme.accent.clone(),
                        fill_style: None,
                    },
                    frame: Rect {
                        x: 200.0,
                        y: 240.0,
                        w: 200.0,
                        h: 100.0,
                    },
                    timing: Timing::default(),
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
            if !has_named(slide, "Drop target") {
                let target_frame = Rect {
                    x: 560.0,
                    y: 210.0,
                    w: 280.0,
                    h: 160.0,
                };
                let id = fresh_id("shape", slide.local_ids());
                slide.objects.push(Object {
                    id,
                    name: "Drop target".to_owned(),
                    kind: ObjectKind::Shape {
                        shape: ShapeKind::RoundedRectangle,
                        fill: theme.accent.clone(),
                        fill_style: None,
                        outline: None,
                        text: None,
                        adjust: Vec::new(),
                    },
                    frame: target_frame,
                    timing: Timing::default(),
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
                let label_id = fresh_id("text", slide.local_ids());
                slide.objects.push(Object {
                    id: label_id,
                    name: "Drop target label".to_owned(),
                    kind: ObjectKind::Text {
                        text: "Drop here".to_owned(),
                        style: coursecraft_course::TextStyle {
                            size: 18.0,
                            bold: true,
                            color: Color("#FFFFFF".to_owned()),
                            align: Align::Center,
                            valign: VAlign::Middle,
                            ..coursecraft_course::TextStyle::default()
                        },
                        runs: Vec::new(),
                        paragraphs: Vec::new(),
                        autofit: Autofit::None,
                        text_style: None,
                    },
                    frame: Rect {
                        x: target_frame.x + 40.0,
                        y: target_frame.y + target_frame.h * 0.5 - 20.0,
                        w: target_frame.w - 80.0,
                        h: 40.0,
                    },
                    timing: Timing::default(),
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
        }
        _ => {}
    }
}

fn submit_button_frame(slide_size: Size) -> Rect {
    const MARGIN: f32 = 40.0;
    const W: f32 = 160.0;
    const H: f32 = 48.0;
    let max_x = (slide_size.w as f32 - MARGIN - W).max(0.0);
    let max_y = (slide_size.h as f32 - MARGIN - H).max(0.0);
    Rect {
        x: max_x,
        y: max_y,
        w: W,
        h: H,
    }
}

fn frames_overlap(a: Rect, b: Rect) -> bool {
    let finite = |r: Rect| r.x.is_finite() && r.y.is_finite() && r.w.is_finite() && r.h.is_finite();
    finite(a)
        && finite(b)
        && a.w > 0.0
        && a.h > 0.0
        && b.w > 0.0
        && b.h > 0.0
        && a.x < b.x + b.w
        && b.x < a.x + a.w
        && a.y < b.y + b.h
        && b.y < a.y + a.h
}

/// Bottom-right Finish button, shifted left of any button it would cover.
fn finish_button_frame(slide: &Slide, slide_size: Size) -> Rect {
    const MARGIN: f32 = 40.0;
    const W: f32 = 160.0;
    const H: f32 = 48.0;
    const GAP: f32 = 12.0;
    let mut x = (slide_size.w as f32 - MARGIN - W).max(0.0);
    let y = (slide_size.h as f32 - MARGIN - H).max(0.0);
    for _ in 0..64 {
        let candidate = Rect { x, y, w: W, h: H };
        let covering = slide
            .objects
            .iter()
            .filter(|object| matches!(object.kind, ObjectKind::Button { .. }))
            .map(|object| object.frame)
            .find(|frame| frames_overlap(candidate, *frame));
        let Some(frame) = covering else {
            break;
        };
        x = (frame.x - GAP - W).max(MARGIN);
    }
    Rect { x, y, w: W, h: H }
}

fn add_finish_button(slide: &mut Slide, theme: &Theme, slide_size: Size) -> String {
    let id = fresh_id("button", slide.local_ids());
    slide.objects.push(Object {
        id: id.clone(),
        name: "Finish".to_owned(),
        kind: ObjectKind::Button {
            label: "Finish".to_owned(),
            fill: theme.accent.clone(),
            fill_style: None,
        },
        frame: finish_button_frame(slide, slide_size),
        timing: Timing::default(),
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
    slide.triggers.push(Trigger {
        action: Action::SubmitResults,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ExitCourse,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    id
}

/// Remove submit-event `submit_question` triggers, which no player control can
/// ever fire, and add a Submit button plus a click trigger that grades the question.
fn add_submit_button(slide: &mut Slide, theme: &Theme, slide_size: Size) -> String {
    let is_dead_submit = |trigger: &Trigger| {
        trigger.action == Action::SubmitQuestion && trigger.when == Event::Submit
    };
    slide.triggers.retain(|trigger| !is_dead_submit(trigger));
    for layer in &mut slide.layers {
        layer.triggers.retain(|trigger| !is_dead_submit(trigger));
    }

    let id = fresh_id("button", slide.local_ids());
    slide.objects.push(Object {
        id: id.clone(),
        name: "Submit".to_owned(),
        kind: ObjectKind::Button {
            label: "Submit".to_owned(),
            fill: theme.accent.clone(),
            fill_style: None,
        },
        frame: submit_button_frame(slide_size),
        timing: Timing::default(),
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
    slide.triggers.push(Trigger {
        action: Action::SubmitQuestion,
        when: Event::Click { object: id.clone() },
        condition: None,
        disabled: false,
    });
    id
}

/// Where a feedback message can sit without covering the question's response
/// rows. Falls back above the prompt when a tall question leaves no room below.
fn feedback_text_frame(slide_size: Size, question: Option<&Question>) -> Rect {
    const MARGIN: f32 = 24.0;
    const ROW_START: f32 = 160.0;
    const ROW_H: f32 = 56.0;
    const ROW_GAP: f32 = 16.0;
    const MIN_H: f32 = 28.0;
    const MIN_GAP: f32 = 4.0;
    let w = (slide_size.w as f32 - 160.0).clamp(0.0, 900.0);
    let slide_h = slide_size.h as f32;
    let below = question.map_or(ROW_START - 20.0, |q| {
        let rows = match q.kind {
            QuestionKind::MultipleChoice
            | QuestionKind::MultipleResponse
            | QuestionKind::TrueFalse
            | QuestionKind::Sequence
            | QuestionKind::Likert => q.choices.len(),
            QuestionKind::FillIn
            | QuestionKind::Matching
            | QuestionKind::ShortAnswer
            | QuestionKind::Essay => q.pairs.len().max(1),
            QuestionKind::Hotspot | QuestionKind::DragDrop => 0,
            QuestionKind::Freeform => 0,
        };
        if rows == 0 {
            ROW_START - 20.0
        } else {
            let available = (slide_h - ROW_START - MARGIN).max(0.0);
            let (row_h, stride) = if (rows as f32) * (ROW_H + ROW_GAP) <= available {
                (ROW_H, ROW_H + ROW_GAP)
            } else {
                let min_stride = MIN_H + MIN_GAP;
                if !available.is_finite() || (rows as f32) * min_stride > available {
                    (MIN_H, min_stride)
                } else {
                    let stride = available / rows as f32;
                    let h = (stride - MIN_GAP).clamp(MIN_H, ROW_H);
                    (h, h + (stride - h).max(0.0))
                }
            };
            ROW_START + stride * (rows.saturating_sub(1)) as f32 + row_h
        }
    });
    let h = 60.0;
    let y = below + 16.0;
    if y + h <= slide_h - MARGIN {
        Rect { x: 80.0, y, w, h }
    } else {
        Rect {
            x: 80.0,
            y: 10.0,
            w,
            h: 40.0,
        }
    }
}

/// Add the two hidden feedback layers and point the question at them.
fn add_feedback_layers(slide: &mut Slide, theme: &Theme, slide_size: Size) -> (String, String) {
    let frame = feedback_text_frame(slide_size, slide.question.as_ref());
    let make_layer = |slide: &mut Slide, name: &str, text: &str| {
        let layer_id = fresh_id(name, slide.local_ids());
        let text_id = fresh_id("feedback_text", slide.local_ids());
        slide.layers.push(Layer {
            id: layer_id.clone(),
            name: name.to_owned(),
            duration_ms: 5000,
            objects: vec![Object {
                id: text_id,
                name: format!("{name} feedback"),
                kind: ObjectKind::Text {
                    text: text.to_owned(),
                    style: coursecraft_course::TextStyle {
                        size: 28.0,
                        bold: true,
                        color: theme.text.clone(),
                        ..coursecraft_course::TextStyle::default()
                    },
                    runs: Vec::new(),
                    paragraphs: Vec::new(),
                    autofit: Autofit::None,
                    text_style: None,
                },
                frame,
                timing: Timing::default(),
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
            }],
            triggers: Vec::new(),
            hide_other_layers: true,
            pause_base_timeline: false,
            prevent_base_clicks: false,
            hide_base_objects: false,
            hide_when_timeline_ends: false,
            allow_seek: SeekMode::Auto,
            revisit: RevisitMode::Auto,
        });
        layer_id
    };
    let correct = make_layer(slide, "Correct", "Correct!");
    let incorrect = make_layer(slide, "Incorrect", "Not quite.");
    if let Some(question) = slide.question.as_mut() {
        question.correct_layer = Some(correct.clone());
        question.incorrect_layer = Some(incorrect.clone());
    }
    (correct, incorrect)
}

/// A button with `label`, filled with the theme accent.
fn results_button(id: String, label: &str, fill: Color, frame: Rect) -> Object {
    Object {
        id,
        name: label.to_owned(),
        kind: ObjectKind::Button {
            label: label.to_owned(),
            fill,
            fill_style: None,
        },
        frame,
        timing: Timing::default(),
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
    }
}

/// One outcome layer of a results slide: a message and two action buttons.
#[allow(clippy::too_many_arguments)]
fn results_layer(
    slide: &Slide,
    layer_id: &str,
    name: &str,
    message: &str,
    message_frame: Rect,
    buttons: [(&str, &str, Rect, Action); 2],
    theme: &Theme,
) -> Layer {
    let message_id = fresh_id("message", slide.local_ids());
    let mut objects = vec![layout_text(
        message_id,
        &format!("{name} message"),
        message.to_owned(),
        28.0,
        message_frame,
        theme,
    )];
    let mut triggers = Vec::new();
    for (prefix, label, frame, action) in buttons {
        let object_id = fresh_id(prefix, slide.local_ids());
        objects.push(results_button(
            object_id.clone(),
            label,
            theme.accent.clone(),
            frame,
        ));
        triggers.push(Trigger {
            action,
            when: Event::Click { object: object_id },
            condition: None,
            disabled: false,
        });
    }
    Layer {
        id: layer_id.to_owned(),
        name: name.to_owned(),
        duration_ms: 5000,
        objects,
        triggers,
        hide_other_layers: true,
        pause_base_timeline: false,
        prevent_base_clicks: false,
        hide_base_objects: false,
        hide_when_timeline_ends: false,
        allow_seek: SeekMode::Auto,
        revisit: RevisitMode::Auto,
    }
}

/// A slide with the score, a Passed layer and a Failed layer.
fn build_results_slide(id: &str, theme: &Theme, size: Size, passing_percent: u8) -> Slide {
    let w = size.w.max(1) as f32;
    let h = size.h.max(1) as f32;
    let frame = |x: f32, y: f32, fw: f32, fh: f32| Rect {
        x: x * w,
        y: y * h,
        w: fw * w,
        h: fh * h,
    };
    let button_width = 160.0;
    let button_height = 48.0;
    let gap = 12.0;
    let margin = 40.0;
    let button_y = (h - margin - button_height).max(0.0);
    let first_x = (w - margin - button_width * 2.0 - gap).max(0.0);
    let second_x = first_x + button_width + gap;

    let mut slide = Slide {
        id: id.to_owned(),
        title: "Results".to_owned(),
        duration_ms: 5000,
        background: None,
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let mut push =
        |prefix: &str, name: &str, text: String, text_size: f32, bold: bool, frame: Rect| {
            let object_id = fresh_id(prefix, slide.local_ids());
            let mut object = layout_text(object_id, name, text, text_size, frame, theme);
            if let ObjectKind::Text { style, .. } = &mut object.kind {
                style.bold = bold;
            }
            slide.objects.push(object);
        };
    push(
        "title",
        "Results title",
        "Results".to_owned(),
        44.0,
        true,
        frame(0.08, 0.10, 0.84, 0.12),
    );
    push(
        "score",
        "Score",
        "Your score: %Course.ScorePercent%%".to_owned(),
        32.0,
        false,
        frame(0.08, 0.30, 0.84, 0.10),
    );
    push(
        "points",
        "Points",
        "Points: %Course.ScorePoints%".to_owned(),
        24.0,
        false,
        frame(0.08, 0.42, 0.84, 0.08),
    );
    push(
        "passing",
        "Passing score",
        format!("Passing score: {passing_percent}%"),
        24.0,
        false,
        frame(0.08, 0.50, 0.84, 0.08),
    );

    let message_frame = frame(0.08, 0.62, 0.84, 0.08);
    let first = Rect {
        x: first_x,
        y: button_y,
        w: button_width,
        h: button_height,
    };
    let second = Rect {
        x: second_x,
        y: button_y,
        w: button_width,
        h: button_height,
    };
    slide.layers.push(results_layer(
        &slide,
        "passed",
        "Passed",
        "Congratulations, you passed.",
        message_frame,
        [
            ("passed_review", "Review quiz", first, Action::ReviewQuiz),
            ("passed_finish", "Finish", second, Action::ExitCourse),
        ],
        theme,
    ));
    slide.layers.push(results_layer(
        &slide,
        "failed",
        "Failed",
        "You did not pass this time.",
        message_frame,
        [
            ("failed_retry", "Retry quiz", first, Action::RetryQuiz),
            ("failed_review", "Review quiz", second, Action::ReviewQuiz),
        ],
        theme,
    ));

    slide.triggers.push(Trigger {
        action: Action::SubmitResults,
        when: Event::TimelineStarts { layer: None },
        condition: None,
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "passed".to_owned(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: Some(Condition::Compare {
            variable: "Quiz.Passed".to_owned(),
            op: CmpOp::Eq,
            value: Value::Bool(true),
        }),
        disabled: false,
    });
    slide.triggers.push(Trigger {
        action: Action::ShowLayer {
            layer: "failed".to_owned(),
        },
        when: Event::TimelineStarts { layer: None },
        condition: Some(Condition::Compare {
            variable: "Quiz.Passed".to_owned(),
            op: CmpOp::Eq,
            value: Value::Bool(false),
        }),
        disabled: false,
    });
    slide
}

fn results_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "results.insert";
    let was_valid = s.course.is_playable();
    let after: Option<String> = opt(C, p, "after")?;
    if !s.course.slides().any(|slide| slide.question.is_some()) {
        return Err(EngineError::Invalid(
            "the course has no question to report".into(),
        ));
    }
    let passing_percent = match s.course.quiz {
        Some(quiz) => quiz.passing_percent,
        None => {
            s.course.quiz = Some(QuizSettings {
                passing_percent: 80,
            });
            80
        }
    };
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let id = fresh_id("results", s.course.all_slide_ids());
    let slide = build_results_slide(&id, &theme, slide_size, passing_percent);
    insert_slide_after(s, slide, None, after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({
        "slide": id,
        "passed_layer": "passed",
        "failed_layer": "failed",
    }))
}

/// A centred text object for the certificate.
fn certificate_text(
    id: String,
    name: &str,
    text: String,
    size: f32,
    bold: bool,
    frame: Rect,
    theme: &Theme,
) -> Object {
    Object {
        id,
        name: name.to_owned(),
        kind: ObjectKind::Text {
            text,
            style: TextStyle {
                font: theme.font.clone(),
                size,
                color: theme.text.clone(),
                bold,
                align: Align::Center,
                valign: VAlign::Middle,
                ..TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
        },
        frame,
        timing: Timing::default(),
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
    }
}

/// A certificate slide: a border, the learner's name, the course and a Print button.
fn build_certificate_slide(id: &str, theme: &Theme, size: Size) -> Slide {
    let w = size.w.max(1) as f32;
    let h = size.h.max(1) as f32;
    let frame = |x: f32, y: f32, fw: f32, fh: f32| Rect {
        x: x * w,
        y: y * h,
        w: fw * w,
        h: fh * h,
    };

    let mut slide = Slide {
        id: id.to_owned(),
        title: "Certificate".to_owned(),
        duration_ms: 5000,
        background: Some(theme.background.clone()),
        objects: Vec::new(),
        layers: Vec::new(),
        triggers: Vec::new(),
        transition: None,
        question: None,
        draw: None,
        notes: String::new(),
        master: None,
        revisit: RevisitMode::Auto,
        player: None,
    };

    let border_id = fresh_id("border", slide.local_ids());
    slide.objects.push(Object {
        id: border_id,
        name: "Border".to_owned(),
        kind: ObjectKind::Shape {
            shape: ShapeKind::RoundedRectangle,
            fill: Color("#00000000".to_owned()),
            fill_style: None,
            outline: Some(Outline {
                color: theme.accent.clone(),
                width: 6.0,
                dash: DashStyle::Solid,
            }),
            text: None,
            adjust: Vec::new(),
        },
        frame: frame(0.05, 0.05, 0.90, 0.90),
        timing: Timing::default(),
        states: Vec::new(),
        initial_state: "normal".into(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        focus_order: None,
        decorative: true,
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

    let mut push = |prefix: &str, name: &str, text: String, size: f32, bold: bool, frame: Rect| {
        let text_id = fresh_id(prefix, slide.local_ids());
        slide.objects.push(certificate_text(
            text_id, name, text, size, bold, frame, theme,
        ));
    };
    push(
        "title",
        "Certificate title",
        "Certificate of Completion".to_owned(),
        44.0,
        true,
        frame(0.10, 0.14, 0.80, 0.12),
    );
    push(
        "certifies",
        "Certifies",
        "This certifies that".to_owned(),
        22.0,
        false,
        frame(0.10, 0.30, 0.80, 0.06),
    );
    push(
        "learner",
        "Learner name",
        "%Learner.Name%".to_owned(),
        36.0,
        true,
        frame(0.10, 0.38, 0.80, 0.07),
    );
    push(
        "completed",
        "Has completed",
        "has completed".to_owned(),
        22.0,
        false,
        frame(0.10, 0.50, 0.80, 0.06),
    );
    push(
        "course",
        "Course title",
        "%Course.Title%".to_owned(),
        30.0,
        true,
        frame(0.10, 0.57, 0.80, 0.07),
    );
    push(
        "score",
        "Score",
        "Score: %Course.ScorePercent%%".to_owned(),
        22.0,
        false,
        frame(0.10, 0.68, 0.80, 0.06),
    );
    push(
        "date",
        "Date",
        "%Course.Date%".to_owned(),
        20.0,
        false,
        frame(0.10, 0.76, 0.80, 0.05),
    );

    let margin = 40.0;
    let button_id = fresh_id("print", slide.local_ids());
    slide.objects.push(results_button(
        button_id.clone(),
        "Print",
        theme.accent.clone(),
        Rect {
            x: (w - margin - 160.0).max(0.0),
            y: (h - margin - 48.0).max(0.0),
            w: 160.0,
            h: 48.0,
        },
    ));
    slide.triggers.push(Trigger {
        action: Action::PrintSlide,
        when: Event::Click { object: button_id },
        condition: None,
        disabled: false,
    });
    slide
}

fn certificate_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "certificate.insert";
    let was_valid = s.course.is_playable();
    let after: Option<String> = opt(C, p, "after")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let id = fresh_id("certificate", s.course.all_slide_ids());
    let slide = build_certificate_slide(&id, &theme, slide_size);
    insert_slide_after(s, slide, None, after.as_deref())?;
    keep_valid(s, was_valid)?;
    Ok(json!({ "slide": id }))
}

fn question_add_feedback_layers(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_feedback_layers";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    let (correct, incorrect) = add_feedback_layers(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "correct": correct, "incorrect": incorrect }))
}

/// A plain text object for an inserted interaction.
fn interaction_text(id: String, name: String, text: String, frame: Rect, theme: &Theme) -> Object {
    Object {
        id,
        name,
        kind: ObjectKind::Text {
            text,
            style: TextStyle {
                size: 24.0,
                color: theme.text.clone(),
                ..TextStyle::default()
            },
            runs: Vec::new(),
            paragraphs: Vec::new(),
            autofit: Autofit::None,
            text_style: None,
        },
        frame,
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: Vec::new(),
        initial_state: "normal".into(),
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

/// A button with a `selected` state in the theme accent.
fn interaction_button(
    id: String,
    name: String,
    label: String,
    frame: Rect,
    selected: bool,
    theme: &Theme,
) -> Object {
    // Button labels are white, so the unselected fill must stay dark enough to read.
    let normal = tint(theme.text.clone(), 0x88);
    Object {
        id,
        name,
        kind: ObjectKind::Button {
            label,
            fill: normal,
            fill_style: None,
        },
        frame,
        timing: Timing::default(),
        entrance: None,
        exit: None,
        motion_paths: Vec::new(),
        states: vec![StateDef {
            name: "selected".into(),
            fill: Some(theme.accent.clone()),
            text: None,
            src: None,
        }],
        initial_state: if selected {
            "selected".into()
        } else {
            "normal".into()
        },
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

/// A theme colour with its alpha replaced, so normal states stay readable on
/// any background without inventing a new palette.
fn tint(color: Color, alpha: u8) -> Color {
    let hex = color.0.strip_prefix('#').unwrap_or("");
    let first_six: String = hex.chars().take(6).collect();
    if first_six.len() == 6 && first_six.chars().all(|c| c.is_ascii_hexdigit()) {
        Color(format!("#{first_six}{alpha:02X}"))
    } else {
        color
    }
}

fn interaction_trigger(action: Action, when: Event) -> Trigger {
    Trigger {
        action,
        when,
        condition: None,
        disabled: false,
    }
}

fn interaction_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "interaction.insert";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    if s.course.play_slide(&sid).is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} is not a playable slide"
        )));
    }
    let kind: String = req(C, p, "kind")?;
    let theme = s.course.theme.clone();
    let size = s.course.slide_size;
    let (w, h) = (size.w as f32, size.h as f32);
    if !w.is_finite() || !h.is_finite() || w <= 0.0 || h <= 0.0 {
        return Err(bad(C, "slide size is invalid"));
    }

    let slide = slide_mut(s, &sid)?;
    let margin = (w * 0.06).clamp(20.0, 120.0);
    let top = (h * 0.12).max(20.0);
    let gap = (w * 0.012).clamp(6.0, 20.0);
    let mut base_ids: Vec<String> = Vec::new();
    let mut layer_ids: Vec<String> = Vec::new();
    let mut all_ids: Vec<String> = Vec::new();

    match kind.as_str() {
        "tabs" => {
            let count = 3usize;
            let tab_h = (h * 0.08).clamp(36.0, 64.0);
            let tab_w = ((w - 2.0 * margin - gap * (count as f32 - 1.0)) / count as f32).max(20.0);
            let content_y = top + tab_h + gap;
            let content_h = (h - content_y - top).max(40.0);
            let mut tab_ids = Vec::new();
            for i in 0..count {
                let x = margin + (tab_w + gap) * i as f32;
                let id = fresh_id("tab_button", slide.local_ids());
                tab_ids.push(id.clone());
                let button = interaction_button(
                    id.clone(),
                    format!("Tab {} button", i + 1),
                    format!("Tab {}", i + 1),
                    Rect {
                        x,
                        y: top,
                        w: tab_w,
                        h: tab_h,
                    },
                    i == 0,
                    &theme,
                );
                slide.objects.push(button);
                all_ids.push(id.clone());
                base_ids.push(id);
            }
            for i in 0..count {
                let layer_id = fresh_id("tab_layer", slide.local_ids());
                layer_ids.push(layer_id.clone());
                let text_id = fresh_id("tab_content", slide.local_ids());
                let text = interaction_text(
                    text_id.clone(),
                    format!("Tab {} content", i + 1),
                    format!("Content for Tab {}", i + 1),
                    Rect {
                        x: margin,
                        y: content_y,
                        w: (w - 2.0 * margin).max(20.0),
                        h: content_h,
                    },
                    &theme,
                );
                all_ids.push(text_id.clone());
                slide.layers.push(Layer {
                    id: layer_id.clone(),
                    name: format!("Tab {} content", i + 1),
                    duration_ms: 5000,
                    objects: vec![text],
                    triggers: Vec::new(),
                    hide_other_layers: true,
                    pause_base_timeline: false,
                    prevent_base_clicks: false,
                    hide_base_objects: false,
                    hide_when_timeline_ends: false,
                    allow_seek: SeekMode::Auto,
                    revisit: RevisitMode::Auto,
                });
            }
            // Clicking a tab shows its layer and moves the selected state.
            for i in 0..count {
                let when = Event::Click {
                    object: tab_ids.get(i).cloned().unwrap_or_default(),
                };
                if let Some(layer) = layer_ids.get(i) {
                    slide.triggers.push(interaction_trigger(
                        Action::ShowLayer {
                            layer: layer.clone(),
                        },
                        when.clone(),
                    ));
                }
                for (j, id) in tab_ids.iter().enumerate() {
                    slide.triggers.push(interaction_trigger(
                        Action::ChangeState {
                            object: id.clone(),
                            state: if j == i { "selected" } else { "normal" }.to_owned(),
                        },
                        when.clone(),
                    ));
                }
            }
            if let Some(first) = layer_ids.first() {
                slide.triggers.push(interaction_trigger(
                    Action::ShowLayer {
                        layer: first.clone(),
                    },
                    Event::TimelineStarts { layer: None },
                ));
            }
        }
        "accordion" => {
            let count = 3usize;
            let header_h = (h * 0.08).clamp(36.0, 64.0);
            let header_w = (w - 2.0 * margin).max(20.0);
            let content_h = (h * 0.18).clamp(70.0, 160.0);
            let base_headers: Vec<Rect> = (0..count)
                .map(|i| Rect {
                    x: margin,
                    y: top + (header_h + gap) * i as f32,
                    w: header_w,
                    h: header_h,
                })
                .collect();
            for i in 0..count {
                layer_ids.push(fresh_id(
                    &format!("accordion_layer{}", i + 1),
                    slide.local_ids(),
                ));
            }
            let mut header_ids = Vec::new();
            for (i, frame) in base_headers.iter().copied().enumerate() {
                let id = fresh_id("accordion_header", slide.local_ids());
                header_ids.push(id.clone());
                slide.objects.push(interaction_button(
                    id.clone(),
                    format!("Header {} button", i + 1),
                    format!("Section {}", i + 1),
                    frame,
                    false,
                    &theme,
                ));
                all_ids.push(id.clone());
                base_ids.push(id);
            }
            for open in 0..count {
                let Some(layer_id) = layer_ids.get(open).cloned() else {
                    continue;
                };
                let mut layer = Layer {
                    id: layer_id.clone(),
                    name: format!("Section {} open", open + 1),
                    duration_ms: 5000,
                    objects: Vec::new(),
                    triggers: Vec::new(),
                    hide_other_layers: true,
                    pause_base_timeline: false,
                    prevent_base_clicks: true,
                    hide_base_objects: true,
                    hide_when_timeline_ends: false,
                    allow_seek: SeekMode::Auto,
                    revisit: RevisitMode::Auto,
                };
                let mut layer_headers = Vec::new();
                for (i, base) in base_headers.iter().copied().enumerate() {
                    let shift = if i > open { content_h + gap } else { 0.0 };
                    let frame = Rect {
                        y: base.y + shift,
                        ..base
                    };
                    let id = fresh_id(
                        &format!("accordion_h{}_{}", open + 1, i + 1),
                        slide.local_ids(),
                    );
                    layer_headers.push((id.clone(), i, frame));
                    layer.objects.push(interaction_button(
                        id.clone(),
                        format!("Header {} button (section {} open)", i + 1, open + 1),
                        format!("Section {}", i + 1),
                        frame,
                        false,
                        &theme,
                    ));
                    all_ids.push(id);
                }
                let content_id = fresh_id("accordion_content", slide.local_ids());
                layer.objects.push(interaction_text(
                    content_id.clone(),
                    format!("Header {} content", open + 1),
                    format!("Content for Section {}", open + 1),
                    Rect {
                        x: margin,
                        y: base_headers
                            .get(open)
                            .map_or(top, |frame| frame.y + header_h + gap * 0.5),
                        w: header_w,
                        h: content_h,
                    },
                    &theme,
                ));
                all_ids.push(content_id);
                for (id, i, _) in &layer_headers {
                    let when = Event::Click { object: id.clone() };
                    if *i == open {
                        layer
                            .triggers
                            .push(interaction_trigger(Action::HideLayer { layer: None }, when));
                    } else if let Some(target) = layer_ids.get(*i) {
                        layer.triggers.push(interaction_trigger(
                            Action::ShowLayer {
                                layer: target.clone(),
                            },
                            when,
                        ));
                    }
                }
                slide.layers.push(layer);
            }
            for (i, id) in header_ids.iter().enumerate() {
                if let Some(layer) = layer_ids.get(i) {
                    slide.triggers.push(interaction_trigger(
                        Action::ShowLayer {
                            layer: layer.clone(),
                        },
                        Event::Click { object: id.clone() },
                    ));
                }
            }
        }
        _ => return Err(bad(C, "kind must be \"tabs\" or \"accordion\"")),
    }

    keep_valid(s, was_valid)?;
    Ok(json!({
        "objects": base_ids,
        "layers": layer_ids,
        "all_objects": all_ids,
    }))
}

/// The condition a new freeform question starts with: the first user variable
/// (if any) compared with zero, else the built-in score percent.
fn freeform_default_condition(course: &Course) -> Condition {
    let variable = course
        .variables
        .first()
        .map_or_else(|| "Course.ScorePercent".to_owned(), |v| v.name.clone());
    Condition::Compare {
        variable,
        op: CmpOp::Ge,
        value: Value::Number(0.0),
    }
}

fn question_insert(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.insert";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let kind: QuestionKind = opt(C, p, "kind")?.unwrap_or(QuestionKind::MultipleChoice);
    let freeform_default = freeform_default_condition(&s.course);
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;

    let slide = slide_mut(s, &sid)?;
    if slide.question.is_some() {
        return Err(EngineError::Invalid(format!(
            "slide {sid:?} already has a question"
        )));
    }
    ensure_question_support(slide, kind, &theme);
    let mut question = default_multiple_choice_question();
    apply_question_kind(&slide.objects, &mut question, kind)?;
    if kind == QuestionKind::Freeform {
        question.correct_when = Some(freeform_default);
    }
    slide.question = Some(question);
    add_feedback_layers(slide, &theme, slide_size);
    let button = add_submit_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

fn question_add_submit_button(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_submit_button";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let theme = s.course.theme.clone();
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    let button = add_submit_button(slide, &theme, slide_size);
    keep_valid(s, was_valid)?;
    Ok(json!({ "button": button }))
}

fn question_set(s: &mut Session, p: &Json) -> Result<Json> {
    let was_valid = s.course.is_playable();
    let sid: String = req("question.set", p, "slide")?;
    let question: Question = req("question.set", p, "question")?;
    let slide = slide_mut(s, &sid)?;
    slide.question = Some(question);
    let graded = slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|l| l.triggers.iter()))
        .any(|t| t.action == Action::SubmitQuestion);
    if !graded {
        slide.triggers.push(Trigger {
            action: Action::SubmitQuestion,
            when: Event::Submit,
            condition: None,
            disabled: false,
        });
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_of<'a>(s: &'a mut Session, sid: &str) -> Result<&'a mut Question> {
    let slide = slide_mut(s, sid)?;
    slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))
}

fn require_kind(q: &Question, kind: QuestionKind, cmd: &str) -> Result<()> {
    if q.kind == kind {
        Ok(())
    } else {
        Err(bad(cmd, format!("{cmd} does not apply to this question")))
    }
}

fn index_of(cmd: &str, p: &Json, len: usize) -> Result<usize> {
    let raw: u64 = req(cmd, p, "index")?;
    let idx = usize::try_from(raw).map_err(|_| bad(cmd, "`index` is out of range"))?;
    if idx >= len {
        return Err(bad(cmd, "`index` is out of range"));
    }
    Ok(idx)
}

fn dest_index(cmd: &str, p: &Json, len: usize) -> Result<usize> {
    let raw: u64 = req(cmd, p, "to")?;
    let idx = usize::try_from(raw).map_err(|_| bad(cmd, "`to` is out of range"))?;
    if idx >= len {
        return Err(bad(cmd, "`to` is out of range"));
    }
    Ok(idx)
}

fn fresh_choice_id(choices: &[Choice]) -> String {
    for ch in 'a'..='z' {
        let id = ch.to_string();
        if !choices.iter().any(|c| c.id == id) {
            return id;
        }
    }
    fresh_id("choice_", choices.iter().map(|c| c.id.as_str()))
}

fn push_choice(choices: &mut Vec<Choice>, label: &str, correct: bool) {
    let n = choices.len().saturating_add(1);
    let id = fresh_choice_id(choices);
    choices.push(Choice {
        id,
        text: format!("{label} {n}"),
        correct,
        points: 0,
    });
}

fn base_object<'a>(slide: &'a Slide, id: &str) -> Option<&'a Object> {
    match slide.object(id) {
        Some((object, None)) => Some(object),
        _ => None,
    }
}

fn clear_kind_payload(q: &mut Question) {
    q.accepted.clear();
    q.pairs.clear();
    q.case_sensitive = false;
    q.hotspot_image = None;
    q.regions.clear();
    q.markers = 1;
    q.drag_items.clear();
    q.drop_targets.clear();
    q.snap_to_center = true;
    q.return_if_wrong = false;
    q.one_per_target = true;
    q.correct_when = None;
}

fn keep_one_correct(choices: &mut [Choice]) {
    let mut seen = false;
    for c in choices.iter_mut() {
        if c.correct && !seen {
            seen = true;
        } else {
            c.correct = false;
        }
    }
    if !seen && let Some(first) = choices.first_mut() {
        first.correct = true;
    }
}

fn keep_some_correct(choices: &mut [Choice]) {
    if choices.iter().any(|c| c.correct) {
        return;
    }
    if let Some(first) = choices.first_mut() {
        first.correct = true;
    }
}

fn apply_question_kind(
    base_objects: &[Object],
    q: &mut Question,
    kind: QuestionKind,
) -> Result<()> {
    if q.kind == kind {
        return Ok(());
    }
    match kind {
        QuestionKind::TrueFalse => {
            q.choices = vec![
                Choice {
                    id: "true".into(),
                    text: "True".into(),
                    correct: true,
                    points: 0,
                },
                Choice {
                    id: "false".into(),
                    text: "False".into(),
                    correct: false,
                    points: 0,
                },
            ];
            clear_kind_payload(q);
        }
        QuestionKind::FillIn => {
            q.choices.clear();
            clear_kind_payload(q);
            q.accepted = vec!["answer".into()];
            q.shuffle = false;
        }
        QuestionKind::Sequence => {
            for c in &mut q.choices {
                c.correct = false;
            }
            clear_kind_payload(q);
            q.shuffle = false;
            while q.choices.len() < 2 {
                push_choice(&mut q.choices, "Item", false);
            }
            if q.choices.len() > 20 {
                q.choices.truncate(20);
            }
        }
        QuestionKind::Matching => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
            q.pairs = vec![
                MatchPair {
                    id: "p1".into(),
                    left: "Left 1".into(),
                    right: "Right 1".into(),
                },
                MatchPair {
                    id: "p2".into(),
                    left: "Left 2".into(),
                    right: "Right 2".into(),
                },
            ];
        }
        QuestionKind::MultipleChoice => {
            clear_kind_payload(q);
            while q.choices.len() < 2 {
                let correct = !q.choices.iter().any(|c| c.correct);
                push_choice(&mut q.choices, "Choice", correct);
            }
            keep_one_correct(&mut q.choices);
        }
        QuestionKind::MultipleResponse => {
            clear_kind_payload(q);
            while q.choices.len() < 2 {
                let correct = !q.choices.iter().any(|c| c.correct);
                push_choice(&mut q.choices, "Choice", correct);
            }
            keep_some_correct(&mut q.choices);
        }
        QuestionKind::Hotspot => {
            let img = base_objects
                .iter()
                .find(|o| matches!(o.kind, ObjectKind::Image { .. }));
            let Some(img) = img else {
                return Err(EngineError::Invalid(
                    "hotspot requires at least one base-layer image on the slide".into(),
                ));
            };
            q.choices.clear();
            clear_kind_payload(q);
            let cx = img.frame.x + img.frame.w * 0.5;
            let cy = img.frame.y + img.frame.h * 0.5;
            let rx = (cx - 60.0).max(0.0);
            let ry = (cy - 40.0).max(0.0);
            q.hotspot_image = Some(img.id.clone());
            q.regions = vec![HotspotRegion {
                id: "r1".into(),
                shape: HotspotShape::Rect,
                x: rx,
                y: ry,
                w: 120.0,
                h: 80.0,
                correct: true,
            }];
            q.markers = 1;
            q.shuffle = false;
        }
        QuestionKind::DragDrop => {
            if base_objects.len() < 2 {
                return Err(EngineError::Invalid(
                    "drag_drop requires at least two base-layer objects on the slide".into(),
                ));
            }
            let by_name = |name: &str| {
                base_objects
                    .iter()
                    .find(|o| o.name == name)
                    .map(|o| o.id.clone())
            };
            let item_id = by_name("Drag me")
                .or_else(|| base_objects.first().map(|o| o.id.clone()))
                .ok_or_else(|| EngineError::Invalid("drag_drop requires a drag item".into()))?;
            let target_id = by_name("Drop target")
                .or_else(|| {
                    base_objects
                        .iter()
                        .map(|o| o.id.clone())
                        .find(|id| id != &item_id)
                })
                .ok_or_else(|| EngineError::Invalid("drag_drop requires a drop target".into()))?;
            q.choices.clear();
            clear_kind_payload(q);
            q.drag_items = vec![DragItem {
                item: item_id,
                target: target_id.clone(),
            }];
            q.drop_targets = vec![target_id];
            q.snap_to_center = true;
            q.return_if_wrong = false;
            q.one_per_target = true;
            q.shuffle = false;
        }
        QuestionKind::Likert => {
            q.choices = vec![
                Choice {
                    id: "1".into(),
                    text: "Strongly disagree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "2".into(),
                    text: "Disagree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "3".into(),
                    text: "Neutral".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "4".into(),
                    text: "Agree".into(),
                    correct: false,
                    points: 0,
                },
                Choice {
                    id: "5".into(),
                    text: "Strongly agree".into(),
                    correct: false,
                    points: 0,
                },
            ];
            clear_kind_payload(q);
            q.shuffle = false;
        }
        QuestionKind::ShortAnswer | QuestionKind::Essay => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
        }
        QuestionKind::Freeform => {
            q.choices.clear();
            clear_kind_payload(q);
            q.shuffle = false;
            q.correct_when = Some(Condition::Compare {
                variable: "Course.ScorePercent".into(),
                op: CmpOp::Ge,
                value: Value::Number(0.0),
            });
        }
    }
    q.kind = kind;
    // Reset scoring if it is no longer allowed for the new kind.
    match q.scoring {
        Scoring::PerAnswer
            if !matches!(
                kind,
                QuestionKind::MultipleChoice
                    | QuestionKind::MultipleResponse
                    | QuestionKind::TrueFalse
            ) =>
        {
            q.scoring = Scoring::AllOrNothing;
        }
        Scoring::Partial
            if !matches!(
                kind,
                QuestionKind::MultipleResponse
                    | QuestionKind::Sequence
                    | QuestionKind::Matching
                    | QuestionKind::Hotspot
                    | QuestionKind::DragDrop
            ) =>
        {
            q.scoring = Scoring::AllOrNothing;
        }
        _ => {}
    }
    Ok(())
}

fn move_to<T>(items: &mut Vec<T>, from: usize, to: usize) {
    if from >= items.len() || to >= items.len() {
        return;
    }
    let item = items.remove(from);
    let dest = to.min(items.len());
    items.insert(dest, item);
}

fn question_set_kind(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_kind";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let kind: QuestionKind = req(C, p, "kind")?;
    let freeform_default = freeform_default_condition(&s.course);
    let slide = slide_mut(s, &sid)?;
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    apply_question_kind(&slide.objects, q, kind)?;
    if kind == QuestionKind::Freeform {
        q.correct_when = Some(freeform_default);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_add_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_accepted";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let value = match opt::<String>(C, p, "text")? {
        Some(t) => text(C, t, "text")?,
        None => "answer".to_owned(),
    };
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        q.accepted.push(value);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_remove_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_accepted";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        let idx = index_of(C, p, q.accepted.len())?;
        q.accepted.remove(idx);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_update_accepted(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_accepted";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let value = text(C, req(C, p, "text")?, "text")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        let idx = index_of(C, p, q.accepted.len())?;
        if let Some(slot) = q.accepted.get_mut(idx) {
            *slot = value;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_set_case_sensitive(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_case_sensitive";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let flag: bool = req(C, p, "case_sensitive")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::FillIn, C)?;
        q.case_sensitive = flag;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_add_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_pair";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let left = match opt::<String>(C, p, "left")? {
        Some(v) => text(C, v, "left")?,
        None => "Left".to_owned(),
    };
    let right = match opt::<String>(C, p, "right")? {
        Some(v) => text(C, v, "right")?,
        None => "Right".to_owned(),
    };
    let id = {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let id = fresh_id("p", q.pairs.iter().map(|pair| pair.id.as_str()));
        q.pairs.push(MatchPair {
            id: id.clone(),
            left,
            right,
        });
        id
    };
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn question_remove_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_pair";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let Some(pos) = q.pairs.iter().position(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        q.pairs.remove(pos);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_update_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_pair";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let left: Option<String> = opt(C, p, "left")?;
    let right: Option<String> = opt(C, p, "right")?;
    if left.is_none() && right.is_none() {
        return Err(bad(C, "missing `left` or `right`"));
    }
    let left = left.map(|v| text(C, v, "left")).transpose()?;
    let right = right.map(|v| text(C, v, "right")).transpose()?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let Some(pair) = q.pairs.iter_mut().find(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        if let Some(v) = left {
            pair.left = v;
        }
        if let Some(v) = right {
            pair.right = v;
        }
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_reorder_pair(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.reorder_pair";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Matching, C)?;
        let to = dest_index(C, p, q.pairs.len())?;
        let Some(from) = q.pairs.iter().position(|pair| pair.id == id) else {
            return Err(EngineError::NotFound(format!("pair {id:?} does not exist")));
        };
        move_to(&mut q.pairs, from, to);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_reorder_sequence(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.reorder_sequence";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    {
        let q = question_of(s, &sid)?;
        require_kind(q, QuestionKind::Sequence, C)?;
        let to = dest_index(C, p, q.choices.len())?;
        let Some(from) = q.choices.iter().position(|c| c.id == id) else {
            return Err(EngineError::NotFound(format!(
                "choice {id:?} does not exist"
            )));
        };
        move_to(&mut q.choices, from, to);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_set_hotspot_image(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_hotspot_image";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let image: String = req(C, p, "image")?;
    let slide = slide_mut(s, &sid)?;
    let obj = base_object(slide, &image)
        .ok_or_else(|| bad(C, format!("image {image:?} must name a base-layer object")))?;
    if !matches!(obj.kind, ObjectKind::Image { .. }) {
        return Err(bad(C, format!("object {image:?} is not an image")));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    q.hotspot_image = Some(image);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_add_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_region";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let shape = opt::<HotspotShape>(C, p, "shape")?.unwrap_or(HotspotShape::Rect);
    let slide_size = s.course.slide_size;
    let slide = slide_mut(s, &sid)?;
    let (default_x, default_y) = if let Some(q) = &slide.question
        && let Some(ref img_id) = q.hotspot_image
        && let Some(img) = base_object(slide, img_id)
    {
        let cx = img.frame.x + img.frame.w * 0.5;
        let cy = img.frame.y + img.frame.h * 0.5;
        ((cx - 60.0).max(0.0), (cy - 40.0).max(0.0))
    } else {
        (
            ((slide_size.w as f32) * 0.5 - 60.0).max(0.0),
            ((slide_size.h as f32) * 0.5 - 40.0).max(0.0),
        )
    };
    let x = opt::<f32>(C, p, "x")?.unwrap_or(default_x);
    let y = opt::<f32>(C, p, "y")?.unwrap_or(default_y);
    let w = opt::<f32>(C, p, "w")?.unwrap_or(120.0);
    let h = opt::<f32>(C, p, "h")?.unwrap_or(80.0);

    if !x.is_finite()
        || !y.is_finite()
        || !w.is_finite()
        || !h.is_finite()
        || x < -100_000.0
        || x > 100_000.0
        || y < -100_000.0
        || y > 100_000.0
        || w <= 0.0
        || w > 100_000.0
        || h <= 0.0
        || h > 100_000.0
    {
        return Err(bad(C, "invalid coordinates or dimensions"));
    }

    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    if q.regions.len() >= 20 {
        return Err(bad(C, "cannot exceed 20 regions"));
    }
    let correct = opt::<bool>(C, p, "correct")?
        .unwrap_or(q.regions.is_empty() || !q.regions.iter().any(|r| r.correct));
    let id = fresh_id("r", q.regions.iter().map(|r| r.id.as_str()));
    q.regions.push(HotspotRegion {
        id: id.clone(),
        shape,
        x,
        y,
        w,
        h,
        correct,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn question_update_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_region";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let shape: Option<HotspotShape> = opt(C, p, "shape")?;
    let x: Option<f32> = opt(C, p, "x")?;
    let y: Option<f32> = opt(C, p, "y")?;
    let w: Option<f32> = opt(C, p, "w")?;
    let h: Option<f32> = opt(C, p, "h")?;
    let correct: Option<bool> = opt(C, p, "correct")?;

    for v in [&x, &y].into_iter().flatten() {
        if !v.is_finite() || *v < -100_000.0 || *v > 100_000.0 {
            return Err(bad(C, "coordinate out of range or not finite"));
        }
    }
    for v in [&w, &h].into_iter().flatten() {
        if !v.is_finite() || *v <= 0.0 || *v > 100_000.0 {
            return Err(bad(C, "dimension must be > 0 and <= 100000"));
        }
    }

    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    let Some(pos) = q.regions.iter().position(|r| r.id == id) else {
        return Err(EngineError::NotFound(format!(
            "region {id:?} does not exist"
        )));
    };
    if correct == Some(false)
        && q.regions[pos].correct
        && q.regions.iter().filter(|r| r.correct).count() <= 1
    {
        return Err(bad(C, "hotspot needs at least one correct region"));
    }
    let region = &mut q.regions[pos];
    if let Some(sh) = shape {
        region.shape = sh;
    }
    if let Some(v) = x {
        region.x = v;
    }
    if let Some(v) = y {
        region.y = v;
    }
    if let Some(v) = w {
        region.w = v;
    }
    if let Some(v) = h {
        region.h = v;
    }
    if let Some(c) = correct {
        region.correct = c;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_remove_region(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_region";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let id: String = req(C, p, "id")?;
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    if q.regions.len() <= 1 {
        return Err(bad(C, "hotspot needs at least one region"));
    }
    let Some(pos) = q.regions.iter().position(|r| r.id == id) else {
        return Err(EngineError::NotFound(format!(
            "region {id:?} does not exist"
        )));
    };
    if q.regions[pos].correct && q.regions.iter().filter(|r| r.correct).count() <= 1 {
        return Err(bad(C, "cannot remove the only correct region"));
    }
    q.regions.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_set_markers(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_markers";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let markers: u32 = req(C, p, "markers")?;
    if !(1..=10).contains(&markers) {
        return Err(bad(C, "markers must be between 1 and 10"));
    }
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::Hotspot, C)?;
    q.markers = markers;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_add_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.add_drag_item";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let target: String = req(C, p, "target")?;
    if item == target {
        return Err(bad(C, "drag item cannot target itself"));
    }
    let slide = slide_mut(s, &sid)?;
    if base_object(slide, &item).is_none() {
        return Err(bad(C, format!("item {item:?} is not a base-layer object")));
    }
    if base_object(slide, &target).is_none() {
        return Err(bad(
            C,
            format!("target {target:?} is not a base-layer object"),
        ));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.len() >= 20 {
        return Err(bad(C, "cannot exceed 20 drag items"));
    }
    if q.drag_items.iter().any(|d| d.item == item) {
        return Err(bad(C, format!("drag item {item:?} already exists")));
    }
    if q.drag_items.iter().any(|d| d.item == target) {
        return Err(bad(C, format!("target {target:?} is a drag item")));
    }
    if q.drop_targets.contains(&item) {
        return Err(bad(C, format!("item {item:?} is already a drop target")));
    }
    if !q.drop_targets.contains(&target) {
        if q.drop_targets.len() >= 20 {
            return Err(bad(C, "cannot exceed 20 drop targets"));
        }
        q.drop_targets.push(target.clone());
    }
    q.drag_items.push(DragItem { item, target });
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_update_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.update_drag_item";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let target: String = req(C, p, "target")?;
    if item == target {
        return Err(bad(C, "drag item cannot target itself"));
    }
    let slide = slide_mut(s, &sid)?;
    if base_object(slide, &target).is_none() {
        return Err(bad(
            C,
            format!("target {target:?} is not a base-layer object"),
        ));
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.iter().any(|d| d.item == target) {
        return Err(bad(C, format!("target {target:?} is a drag item")));
    }
    let Some(drag) = q.drag_items.iter_mut().find(|d| d.item == item) else {
        return Err(EngineError::NotFound(format!(
            "drag item {item:?} does not exist"
        )));
    };
    drag.target = target.clone();
    if !q.drop_targets.contains(&target) {
        if q.drop_targets.len() >= 20 {
            return Err(bad(C, "cannot exceed 20 drop targets"));
        }
        q.drop_targets.push(target);
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_remove_drag_item(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.remove_drag_item";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let item: String = req(C, p, "item")?;
    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if q.drag_items.len() <= 1 {
        return Err(bad(C, "drag_drop needs at least one drag item"));
    }
    let Some(pos) = q.drag_items.iter().position(|d| d.item == item) else {
        return Err(EngineError::NotFound(format!(
            "drag item {item:?} does not exist"
        )));
    };
    q.drag_items.remove(pos);
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_set_drop_targets(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_drop_targets";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let targets: Vec<String> = req(C, p, "targets")?;
    if targets.is_empty() || targets.len() > 20 {
        return Err(bad(C, "drop_targets must have 1 to 20 targets"));
    }
    let mut set = std::collections::BTreeSet::new();
    for t in &targets {
        if !set.insert(t.as_str()) {
            return Err(bad(C, format!("duplicate drop target {t:?}")));
        }
    }
    let slide = slide_mut(s, &sid)?;
    for t in &targets {
        if base_object(slide, t).is_none() {
            return Err(bad(
                C,
                format!("drop target {t:?} is not a base-layer object"),
            ));
        }
    }
    let q = slide
        .question
        .as_mut()
        .ok_or_else(|| EngineError::NotFound(format!("slide {sid:?} has no question")))?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    for d in &q.drag_items {
        if targets.contains(&d.item) {
            return Err(bad(
                C,
                format!(
                    "object {:?} cannot be both drag item and drop target",
                    d.item
                ),
            ));
        }
        if !targets.contains(&d.target) {
            return Err(bad(
                C,
                format!(
                    "target {:?} used by drag item {:?} must be in drop_targets",
                    d.target, d.item
                ),
            ));
        }
    }
    q.drop_targets = targets;
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn question_set_drag_options(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.set_drag_options";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let snap_to_center: Option<bool> = opt(C, p, "snap_to_center")?;
    let return_if_wrong: Option<bool> = opt(C, p, "return_if_wrong")?;
    let one_per_target: Option<bool> = opt(C, p, "one_per_target")?;

    let q = question_of(s, &sid)?;
    require_kind(q, QuestionKind::DragDrop, C)?;
    if let Some(v) = snap_to_center {
        q.snap_to_center = v;
    }
    if let Some(v) = return_if_wrong {
        q.return_if_wrong = v;
    }
    if let Some(v) = one_per_target {
        q.one_per_target = v;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// True when a trigger only makes sense on a slide that has a question.
///
/// Validation rejects `submit_question` on a slide without a question, and a
/// `submit` event can never fire once the question is gone.
fn trigger_needs_question(t: &Trigger) -> bool {
    t.action == Action::SubmitQuestion || t.when == Event::Submit
}

fn condition_references_object(c: &Condition, object: &str) -> bool {
    match c {
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(|c| condition_references_object(c, object))
        }
        Condition::Not(inner) => condition_references_object(inner, object),
        Condition::Compare { .. } => false,
        Condition::ObjectState { object: target, .. } => target == object,
    }
}

/// Every object a trigger names in its event, action or condition.
fn trigger_references_object(t: &Trigger, object: &str) -> bool {
    let in_event = match &t.when {
        Event::Click { object: target }
        | Event::HoverStart { object: target }
        | Event::HoverEnd { object: target }
        | Event::MediaEnds { object: target }
        | Event::StateChanges { object: target, .. }
        | Event::AnimationCompletes { object: target, .. } => target == object,
        _ => false,
    };
    let in_action = match &t.action {
        Action::ChangeState { object: target, .. }
        | Action::PlayMedia { object: target }
        | Action::PauseMedia { object: target }
        | Action::StopMedia { object: target }
        | Action::MoveOnPath { object: target, .. } => target == object,
        _ => false,
    };
    in_event
        || in_action
        || t.condition
            .as_ref()
            .is_some_and(|c| condition_references_object(c, object))
}

/// The object id a trigger's event fires from, when the event has one.
fn event_object(event: &Event) -> Option<&str> {
    match event {
        Event::Click { object }
        | Event::HoverStart { object }
        | Event::HoverEnd { object }
        | Event::MediaEnds { object }
        | Event::StateChanges { object, .. }
        | Event::AnimationCompletes { object, .. } => Some(object),
        _ => None,
    }
}

/// Remove a question and everything that cannot exist without it, in one edit:
/// submit triggers, dead submit events, and Submit buttons that have nothing
/// else to do. Layers, feedback layers and all other objects stay.
fn question_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "question.delete";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let slide = slide_mut(s, &sid)?;
    if slide.question.is_none() {
        return Err(EngineError::NotFound(format!(
            "slide {sid:?} has no question"
        )));
    }
    slide.question = None;

    // Objects named by submit_question triggers may have been there only to
    // submit. Remember them before the triggers go.
    let mut submit_objects: BTreeSet<String> = BTreeSet::new();
    for trigger in slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
    {
        if trigger.action == Action::SubmitQuestion
            && let Some(object) = event_object(&trigger.when)
        {
            submit_objects.insert(object.to_owned());
        }
    }

    slide.triggers.retain(|t| !trigger_needs_question(t));
    for layer in &mut slide.layers {
        layer.triggers.retain(|t| !trigger_needs_question(t));
    }

    // A button that had a submit_question trigger and now has no trigger (and
    // is not named by any other trigger) was only there to submit.
    let mut lonely_buttons: BTreeSet<String> = BTreeSet::new();
    let all_objects = slide
        .objects
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.objects.iter()));
    for object in all_objects {
        if !submit_objects.contains(&object.id) || !matches!(object.kind, ObjectKind::Button { .. })
        {
            continue;
        }
        let referenced = slide
            .triggers
            .iter()
            .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()));
        if !referenced
            .into_iter()
            .any(|t| trigger_references_object(t, &object.id))
        {
            lonely_buttons.insert(object.id.clone());
        }
    }
    slide
        .objects
        .retain(|object| !lonely_buttons.contains(&object.id));
    for layer in &mut slide.layers {
        layer
            .objects
            .retain(|object| !lonely_buttons.contains(&object.id));
    }

    // Feedback layers made for this question, if nobody edited them or points at them.
    let unused_feedback: Vec<String> = slide
        .layers
        .iter()
        .filter(|layer| is_untouched_feedback_layer(layer))
        .map(|layer| layer.id.clone())
        .filter(|id| {
            !slide
                .triggers
                .iter()
                .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
                .any(|t| {
                    matches!(&t.action, Action::ShowLayer { layer } if layer == id)
                        || matches!(&t.action, Action::HideLayer { layer: Some(layer) } if layer == id)
                })
        })
        .collect();
    slide
        .layers
        .retain(|layer| !unused_feedback.contains(&layer.id));

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

/// A `Correct` or `Incorrect` layer still exactly as `add_feedback_layers` made it.
fn is_untouched_feedback_layer(layer: &Layer) -> bool {
    let expected = match layer.name.as_str() {
        "Correct" => "Correct!",
        "Incorrect" => "Not quite.",
        _ => return false,
    };
    layer.triggers.is_empty()
        && matches!(
            layer.objects.as_slice(),
            [Object { kind: ObjectKind::Text { text, .. }, .. }] if text == expected
        )
}

fn object_set_animation(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.set_animation";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let which: String = req(C, p, "which")?;
    if which != "entrance" && which != "exit" {
        return Err(bad(
            C,
            format!("which must be 'entrance' or 'exit', got {which:?}"),
        ));
    }
    let map = obj(C, p)?;
    let anim_val = map
        .get("animation")
        .ok_or_else(|| bad(C, "missing `animation`"))?;
    let anim: Option<Animation> = match anim_val {
        Json::Null => None,
        val => {
            let a: Animation = serde_json::from_value(val.clone())
                .map_err(|e| bad(C, format!("invalid animation: {e}")))?;
            if a.duration_ms < 1 || a.duration_ms > 60000 {
                return Err(bad(C, "animation duration_ms must be 1..=60000"));
            }
            Some(a)
        }
    };
    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    if which == "entrance" {
        target.entrance = anim;
    } else {
        target.exit = anim;
    }
    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_add_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.add_motion_path";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let points: Option<Vec<Point>> = opt(C, p, "points")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let easing: Option<Easing> = opt(C, p, "easing")?;
    let relative: Option<bool> = opt(C, p, "relative")?;

    let points =
        points.unwrap_or_else(|| vec![Point { x: 0.0, y: 0.0 }, Point { x: 200.0, y: 0.0 }]);
    if points.len() < 2 || points.len() > 100 {
        return Err(bad(C, "motion path points must be 2..=100"));
    }
    for pt in &points {
        if !pt.x.is_finite() || !pt.y.is_finite() || pt.x.abs() > 100000.0 || pt.y.abs() > 100000.0
        {
            return Err(bad(
                C,
                "motion path points must be finite and within ±100000",
            ));
        }
    }
    let duration_ms = duration_ms.unwrap_or(1000);
    if !(1..=60000).contains(&duration_ms) {
        return Err(bad(C, "motion path duration_ms must be 1..=60000"));
    }
    let easing = easing.unwrap_or_default();
    let relative = relative.unwrap_or(true);

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    if target.motion_paths.len() >= 20 {
        return Err(bad(C, "object cannot have more than 20 motion paths"));
    }

    let next_num = (1..)
        .find(|n| {
            let candidate = format!("path{n}");
            !target.motion_paths.iter().any(|mp| mp.id == candidate)
        })
        .unwrap_or(1);
    let id = format!("path{next_num}");

    target.motion_paths.push(MotionPath {
        id: id.clone(),
        points,
        duration_ms,
        easing,
        relative,
    });
    keep_valid(s, was_valid)?;
    Ok(json!({ "id": id }))
}

fn object_update_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.update_motion_path";
    let was_valid = s.course.is_playable();
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let path_id: String = req(C, p, "path")?;
    let points: Option<Vec<Point>> = opt(C, p, "points")?;
    let duration_ms: Option<u32> = opt(C, p, "duration_ms")?;
    let easing: Option<Easing> = opt(C, p, "easing")?;
    let relative: Option<bool> = opt(C, p, "relative")?;

    if let Some(ref pts) = points {
        if pts.len() < 2 || pts.len() > 100 {
            return Err(bad(C, "motion path points must be 2..=100"));
        }
        for pt in pts {
            if !pt.x.is_finite()
                || !pt.y.is_finite()
                || pt.x.abs() > 100000.0
                || pt.y.abs() > 100000.0
            {
                return Err(bad(
                    C,
                    "motion path points must be finite and within ±100000",
                ));
            }
        }
    }
    if let Some(d) = duration_ms
        && !(1..=60000).contains(&d)
    {
        return Err(bad(C, "motion path duration_ms must be 1..=60000"));
    }

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    let path = target
        .motion_paths
        .iter_mut()
        .find(|mp| mp.id == path_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "motion path {path_id:?} not found on object {oid:?}"
            ))
        })?;

    if let Some(pts) = points {
        path.points = pts;
    }
    if let Some(d) = duration_ms {
        path.duration_ms = d;
    }
    if let Some(e) = easing {
        path.easing = e;
    }
    if let Some(r) = relative {
        path.relative = r;
    }

    keep_valid(s, was_valid)?;
    Ok(Json::Null)
}

fn object_remove_motion_path(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "object.remove_motion_path";
    let sid: String = req(C, p, "slide")?;
    let oid: String = req(C, p, "object")?;
    let path_id: String = req(C, p, "path")?;

    let slide = slide_mut(s, &sid)?;
    let target = find_object_mut(slide, &oid)?;
    let pos = target
        .motion_paths
        .iter()
        .position(|mp| mp.id == path_id)
        .ok_or_else(|| {
            EngineError::NotFound(format!(
                "motion path {path_id:?} not found on object {oid:?}"
            ))
        })?;
    target.motion_paths.remove(pos);

    let mut referencing = Vec::new();
    let slide = slide_mut(s, &sid)?;
    for (i, t) in slide.triggers.iter().enumerate() {
        if trigger_references_path(t, &oid, &path_id) {
            referencing.push(format!("slide:{sid}:trigger:{i}"));
        }
    }
    for l in &slide.layers {
        for (i, t) in l.triggers.iter().enumerate() {
            if trigger_references_path(t, &oid, &path_id) {
                referencing.push(format!("slide:{sid}:layer:{}:trigger:{i}", l.id));
            }
        }
    }

    Ok(json!({ "triggers": referencing }))
}

fn trigger_references_path(t: &Trigger, obj_id: &str, path_id: &str) -> bool {
    if let Action::MoveOnPath {
        ref object,
        ref path,
    } = t.action
        && object == obj_id
        && path == path_id
    {
        return true;
    }
    if let Event::AnimationCompletes {
        ref object,
        path: Some(ref path),
    } = t.when
        && object == obj_id
        && path == path_id
    {
        return true;
    }
    false
}
