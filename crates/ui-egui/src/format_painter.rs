//! Format painter: copy the look of one object onto others.
//!
//! The painter only remembers where the source lives. Applying it dispatches
//! `object.apply_style`, so the change is one undo step.

use egui::{Key, KeyboardShortcut, Modifiers};
use serde_json::json;

use crate::Editor;

/// What an armed painter copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PainterKind {
    /// The look (`object.apply_style`).
    #[default]
    Format,
    /// Entrance, exit and emphasis (`object.copy_animation`, Animations › Animation Painter).
    Animation,
}

/// An armed format painter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatPainter {
    pub source_slide: String,
    pub source: String,
    pub kind: PainterKind,
    /// Stays armed after an application.
    pub sticky: bool,
    /// Suppresses the release `clicked()` of the double-click that armed sticky mode.
    pub(crate) just_double_clicked: bool,
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
        self.arm_painter(PainterKind::Format, sticky);
    }

    /// Arm the Animation Painter from the selected object.
    pub fn arm_animation_painter(&mut self, sticky: bool) {
        self.arm_painter(PainterKind::Animation, sticky);
    }

    /// Whether the armed painter (if any) is the Animation Painter.
    #[must_use]
    pub fn animation_painter_armed(&self) -> bool {
        self.ui
            .format_painter
            .as_ref()
            .is_some_and(|p| p.kind == PainterKind::Animation)
    }

    fn arm_painter(&mut self, kind: PainterKind, sticky: bool) {
        if !self.can_arm_format_painter() {
            return;
        }
        crate::canvas::commit_inline_edit(self);
        let Some(source) = self.selection.object.clone() else {
            return;
        };
        self.ui.format_painter = Some(FormatPainter {
            source_slide: self.selection.slide.clone(),
            source,
            kind,
            sticky,
            just_double_clicked: false,
        });
    }

    /// Apply the armed painter to `targets` on the current slide, disarming
    /// unless it is sticky.
    pub fn apply_format_painter(&mut self, targets: &[String]) {
        let Some(painter) = self.ui.format_painter.clone() else {
            return;
        };
        // Only the source itself is skipped: a duplicated slide holds objects with the same id
        // (`N91`).
        let same_slide = self.selection.slide == painter.source_slide;
        let targets: Vec<&String> = targets
            .iter()
            .filter(|id| !(same_slide && **id == painter.source))
            .collect();
        if !targets.is_empty() {
            match painter.kind {
                PainterKind::Format => self.run(
                    "object.apply_style",
                    json!({
                        "slide": self.selection.slide,
                        "source_slide": painter.source_slide,
                        "source": painter.source,
                        "objects": targets,
                    }),
                ),
                PainterKind::Animation => self.run(
                    "object.copy_animation",
                    json!({
                        "slide": self.selection.slide,
                        "from_slide": painter.source_slide,
                        "from": painter.source,
                        "objects": targets,
                    }),
                ),
            };
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
        // The painter styles slide objects: not in a slide master, in Edit-State Mode or
        // behind a window or the Start page (`N91`).
        if self.ui.master_view || self.ui.editing_state.is_some() || self.edit_keys_blocked() {
            return;
        }
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

    #[test]
    fn user_click_shape_paint_then_click_with_jitter_copies_format() {
        let (mut h, t) = setup();
        // 1. Select shape "a" by clicking it on the canvas.
        h.click_at(t.to_screen_pos(pos2(200.0, 160.0)));
        assert_eq!(h.editor.selection.object.as_deref(), Some("a"));

        // 2. Click the "Paint" button in the toolbar.
        let paint = h.find_text("Paint").expect("Paint button in toolbar");
        h.click_at(paint);
        assert!(h.editor.ui.format_painter.is_some());
        assert!(
            h.find_text("Click an object to copy the format onto it (Esc to cancel)")
                .is_some(),
            "status hint should be visible while paint mode is on"
        );

        // 3. Click shape "b" on the canvas with a realistic 4px pointer jitter
        //    between press and release.
        let target = t.to_screen_pos(pos2(700.0, 360.0));
        h.frame(vec![egui::Event::PointerMoved(target)]);
        h.frame(vec![egui::Event::PointerButton {
            pos: target,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        }]);
        let jittered = pos2(target.x + 4.0, target.y + 2.0);
        h.frame(vec![egui::Event::PointerMoved(jittered)]);
        h.frame(vec![egui::Event::PointerButton {
            pos: jittered,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        }]);
        h.idle_frame();

        assert_eq!(
            fill_of(&h, "b"),
            "#112233",
            "target should receive source fill"
        );
        assert!(h.editor.ui.format_painter.is_none());
    }

    #[test]
    fn paint_mode_shows_copy_cursor_on_canvas_and_double_click_is_sticky() {
        let (mut h, t) = setup();
        h.click_at(t.to_screen_pos(pos2(200.0, 160.0)));
        let paint = h.find_text("Paint").expect("Paint button in toolbar");
        // Double-click Paint to keep it armed for multiple objects.
        let click = |h: &mut TestHarness, time: f64| {
            for (t, ev) in [
                (time, egui::Event::PointerMoved(paint)),
                (
                    time + 0.01,
                    egui::Event::PointerButton {
                        pos: paint,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::NONE,
                    },
                ),
                (
                    time + 0.02,
                    egui::Event::PointerButton {
                        pos: paint,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    },
                ),
            ] {
                let output = h.ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(h.screen_rect),
                        time: Some(t),
                        events: vec![ev],
                        ..Default::default()
                    },
                    |ui| {
                        let _ = h.editor.show(ui);
                    },
                );
                output.drop_without_applying_deltas();
            }
        };
        click(&mut h, 1.0);
        assert!(h.editor.ui.format_painter.is_some(), "first click arms");
        click(&mut h, 1.1);
        assert!(
            h.editor
                .ui
                .format_painter
                .as_ref()
                .is_some_and(|p| p.sticky),
            "double-clicking Paint should arm sticky mode, got: {:?}",
            h.editor.ui.format_painter
        );

        // Hovering the canvas in paint mode shows the Copy cursor.
        let canvas_pt = t.to_screen_pos(pos2(700.0, 360.0));
        let output = h.ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(h.screen_rect),
                events: vec![egui::Event::PointerMoved(canvas_pt)],
                ..Default::default()
            },
            |ui| {
                let _ = h.editor.show(ui);
            },
        );
        assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::Copy);
        output.drop_without_applying_deltas();

        // Click two targets in sticky mode, then cancel with Esc.
        h.click_at(t.to_screen_pos(pos2(700.0, 360.0)));
        h.click_at(t.to_screen_pos(pos2(1000.0, 560.0)));
        assert_eq!(fill_of(&h, "b"), "#112233");
        assert_eq!(fill_of(&h, "c"), "#112233");
        assert!(h.editor.ui.format_painter.is_some());
        h.press(Key::Escape, Modifiers::NONE);
        assert!(h.editor.ui.format_painter.is_none());
    }

    #[test]
    fn text_box_to_text_box_and_shape_outline_copy_and_undo() {
        let json = serde_json::json!({
            "schema_version": 1, "id": "fp2", "title": "FP2",
            "slide_size": { "w": 1280, "h": 720 },
            "scenes": [{ "id": "scene1", "title": "S", "slides": [{
                "id": "slide1", "title": "One",
                "objects": [
                    { "id": "t1", "kind": { "type": "text", "text": "Source",
                        "style": { "size": 32, "bold": true, "color": "#AA2244" } },
                      "frame": { "x": 100, "y": 100, "w": 240, "h": 100 } },
                    { "id": "t2", "kind": { "type": "text", "text": "Target",
                        "style": { "size": 14, "bold": false, "color": "#000000" } },
                      "frame": { "x": 500, "y": 100, "w": 240, "h": 100 } },
                    { "id": "s1", "kind": { "type": "shape", "shape": "rectangle",
                        "fill": "#123456",
                        "outline": { "color": "#FEDCBA", "width": 4.0 } },
                      "frame": { "x": 100, "y": 350, "w": 200, "h": 120 } },
                    { "id": "s2", "kind": { "type": "shape", "shape": "ellipse",
                        "fill": "#FFFFFF" },
                      "frame": { "x": 500, "y": 350, "w": 200, "h": 120 } }
                ]
            }]}]
        });
        let course = Course::from_json(&json.to_string()).expect("course");
        let mut h = TestHarness::new(course);
        h.editor.selection.slide = "slide1".into();
        h.editor.ui.snap_to_guides = false;
        h.idle_frame();
        let t = SlideTransform::fit(
            canvas_fit_rect(canvas_rect(h.central_panel_rect())),
            h.editor.session.course.slide_size,
        )
        .expect("transform");

        // Text box to text box
        h.click_at(t.to_screen_pos(pos2(220.0, 150.0)));
        let paint = h.find_text("Paint").expect("Paint button");
        h.click_at(paint);
        h.click_at(t.to_screen_pos(pos2(620.0, 150.0)));
        let t2 = &h
            .editor
            .session
            .course
            .slide("slide1")
            .expect("slide")
            .object("t2")
            .expect("t2")
            .0
            .kind;
        match t2 {
            ObjectKind::Text { text, style, .. } => {
                assert_eq!(text, "Target");
                assert!((style.size - 32.0).abs() < 0.01);
                assert!(style.bold);
                assert_eq!(style.color.0, "#AA2244");
            }
            other => panic!("{other:?}"),
        }

        // Undo restores t2
        h.editor.session.undo().expect("undo");
        let t2_after = &h
            .editor
            .session
            .course
            .slide("slide1")
            .expect("slide")
            .object("t2")
            .expect("t2")
            .0
            .kind;
        match t2_after {
            ObjectKind::Text { style, .. } => {
                assert!((style.size - 14.0).abs() < 0.01);
                assert!(!style.bold);
            }
            other => panic!("{other:?}"),
        }

        // Shape fill + outline to another shape
        h.click_at(t.to_screen_pos(pos2(200.0, 410.0)));
        let paint = h.find_text("Paint").expect("Paint button");
        h.click_at(paint);
        h.click_at(t.to_screen_pos(pos2(600.0, 410.0)));
        let s2 = &h
            .editor
            .session
            .course
            .slide("slide1")
            .expect("slide")
            .object("s2")
            .expect("s2")
            .0
            .kind;
        match s2 {
            ObjectKind::Shape { fill, outline, .. } => {
                assert_eq!(fill.0, "#123456");
                assert_eq!(
                    outline.as_ref().map(|o| (o.color.0.as_str(), o.width)),
                    Some(("#FEDCBA", 4.0))
                );
            }
            other => panic!("{other:?}"),
        }
    }
}
