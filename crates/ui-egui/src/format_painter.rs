//! Format painter: copy the look of one object onto others.
//!
//! The painter only remembers where the source lives. Applying it dispatches
//! `object.apply_style`, so the change is one undo step.

use egui::{Key, KeyboardShortcut, Modifiers};
use serde_json::json;

use crate::Editor;

/// An armed format painter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatPainter {
    pub source_slide: String,
    pub source: String,
    /// Stays armed after an application.
    pub sticky: bool,
}

/// Cmd/Ctrl+Alt+C arms the painter from the selection.
pub const ARM_SHORTCUT: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::C);
/// Cmd/Ctrl+Alt+V applies it to the selection.
pub const APPLY_SHORTCUT: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::V);

impl Editor {
    /// Whether exactly one object is selected, so it can be a painter source.
    #[must_use]
    pub fn can_arm_format_painter(&self) -> bool {
        self.selection.object.is_some()
            && self.selection.also.is_empty()
            && !self.selection.slide.is_empty()
    }

    /// Arm the painter from the selected object. Does nothing without one.
    pub fn arm_format_painter(&mut self, sticky: bool) {
        if !self.can_arm_format_painter() {
            return;
        }
        let Some(source) = self.selection.object.clone() else {
            return;
        };
        self.ui.format_painter = Some(FormatPainter {
            source_slide: self.selection.slide.clone(),
            source,
            sticky,
        });
    }

    /// Apply the armed painter to `targets` on the current slide, disarming
    /// unless it is sticky.
    pub fn apply_format_painter(&mut self, targets: &[String]) {
        let Some(painter) = self.ui.format_painter.clone() else {
            return;
        };
        let targets: Vec<&String> = targets.iter().filter(|id| **id != painter.source).collect();
        if !targets.is_empty() {
            self.run(
                "object.apply_style",
                json!({
                    "slide": self.selection.slide,
                    "source_slide": painter.source_slide,
                    "source": painter.source,
                    "objects": targets,
                }),
            );
        }
        if !painter.sticky {
            self.ui.format_painter = None;
        }
    }

    /// Cmd/Ctrl+Alt+V: apply the armed painter to the current selection.
    pub fn apply_format_painter_to_selection(&mut self) {
        let ids = self.selection.ids();
        self.apply_format_painter(&ids);
    }

    /// Escape disarms before the canvas sees the key.
    pub fn consume_format_painter_escape(&mut self, ctx: &egui::Context) {
        if self.ui.format_painter.is_some()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.ui.format_painter = None;
        }
    }

    /// Cmd/Ctrl+Alt+C and +V.
    pub fn handle_format_painter_shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.input_mut(|i| i.consume_shortcut(&ARM_SHORTCUT)) {
            self.arm_format_painter(false);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&APPLY_SHORTCUT)) {
            self.apply_format_painter_to_selection();
        }
    }
}

#[cfg(test)]
mod tests {
    use coursecraft_course::{Course, ObjectKind};
    use egui::{Key, Modifiers, pos2};

    use crate::canvas::{canvas_fit_rect, canvas_rect};
    use crate::test_harness::TestHarness;
    use coursecraft_render::{Icon, SlideTransform};

    fn course() -> Course {
        let json = serde_json::json!({
            "schema_version": 1, "id": "fp", "title": "FP",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "scene1", "title": "S", "slides": [{
                "id": "slide1", "title": "One",
                "objects": [
                    { "id": "a", "kind": { "type": "shape", "shape": "rectangle", "fill": "#112233" },
                      "frame": { "x": 100, "y": 100, "w": 200, "h": 120 } },
                    { "id": "b", "kind": { "type": "shape", "shape": "rectangle", "fill": "#FF0000" },
                      "frame": { "x": 600, "y": 300, "w": 200, "h": 120 } },
                    { "id": "c", "kind": { "type": "shape", "shape": "rectangle", "fill": "#00FF00" },
                      "frame": { "x": 900, "y": 500, "w": 200, "h": 120 } }
                ]
            }]}]
        });
        Course::from_json(&json.to_string()).expect("course")
    }

    fn fill_of(h: &TestHarness, id: &str) -> String {
        let slide = h.editor.session.course.slide("slide1").expect("slide");
        match &slide.object(id).expect("object").0.kind {
            ObjectKind::Shape { fill, .. } => fill.0.clone(),
            other => panic!("{other:?}"),
        }
    }

    fn setup() -> (TestHarness, SlideTransform) {
        let mut h = TestHarness::new(course());
        h.editor.selection.slide = "slide1".into();
        h.editor.ui.snap_to_guides = false;
        h.idle_frame();
        let t = SlideTransform::fit(
            canvas_fit_rect(canvas_rect(h.central_panel_rect())),
            h.editor.session.course.slide_size,
        )
        .expect("transform");
        (h, t)
    }

    #[test]
    fn arm_click_target_applies_and_disarms() {
        let (mut h, t) = setup();
        h.editor.selection.set_many(["a"]);
        h.idle_frame();
        let brush = h.find_icon(Icon::PaintBrush).expect("toolbar button");
        h.click_at(brush);
        assert!(h.editor.ui.format_painter.is_some());
        h.click_at(t.to_screen_pos(pos2(700.0, 360.0)));
        assert_eq!(fill_of(&h, "b"), "#112233");
        assert_eq!(fill_of(&h, "c"), "#00FF00");
        assert!(h.editor.ui.format_painter.is_none());
    }

    #[test]
    fn sticky_painter_keeps_applying() {
        let (mut h, t) = setup();
        h.editor.selection.set_many(["a"]);
        h.editor.arm_format_painter(true);
        h.idle_frame();
        h.click_at(t.to_screen_pos(pos2(700.0, 360.0)));
        h.click_at(t.to_screen_pos(pos2(1000.0, 560.0)));
        assert_eq!(fill_of(&h, "b"), "#112233");
        assert_eq!(fill_of(&h, "c"), "#112233");
        assert!(h.editor.ui.format_painter.is_some());
        h.press(Key::Escape, Modifiers::NONE);
        assert!(h.editor.ui.format_painter.is_none());
    }

    #[test]
    fn escape_disarms_and_arming_needs_one_object() {
        let (mut h, _t) = setup();
        h.editor.arm_format_painter(false);
        assert!(h.editor.ui.format_painter.is_none(), "nothing selected");
        h.editor.selection.set_many(["a", "b"]);
        h.editor.arm_format_painter(false);
        assert!(h.editor.ui.format_painter.is_none(), "two selected");
        h.editor.selection.set_many(["a"]);
        h.editor.arm_format_painter(false);
        assert!(h.editor.ui.format_painter.is_some());
        h.press(Key::Escape, Modifiers::NONE);
        assert!(h.editor.ui.format_painter.is_none());
        assert_eq!(h.editor.selection.object.as_deref(), Some("a"));
    }

    #[test]
    fn apply_to_selection_is_undoable() {
        let (mut h, _t) = setup();
        h.editor.selection.set_many(["a"]);
        h.editor.arm_format_painter(false);
        h.editor.selection.set_many(["b", "c"]);
        h.editor.apply_format_painter_to_selection();
        assert_eq!(fill_of(&h, "b"), "#112233");
        assert_eq!(fill_of(&h, "c"), "#112233");
        h.editor.session.undo().expect("undo");
        assert_eq!(fill_of(&h, "b"), "#FF0000");
    }
}
