//! Shared test harness for `coursecraft-ui-egui` headless UI tests.

use crate::*;
use coursecraft_course::Course;
use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

pub(crate) fn text_center(t: &egui::epaint::TextShape) -> Pos2 {
    let r = t.visual_bounding_rect();
    if r.is_positive() && r.center().x.is_finite() && r.center().y.is_finite() {
        r.center()
    } else {
        t.galley.rect.translate(t.pos.to_vec2()).center()
    }
}

pub struct TestHarness {
    pub ctx: Context,
    pub editor: Editor,
    pub screen_rect: Rect,
}

#[allow(dead_code)]
impl TestHarness {
    pub fn new(course: Course) -> Self {
        Self::new_with_size(course, (1280.0, 900.0))
    }

    pub fn new_with_size(course: Course, (w, h): (f32, f32)) -> Self {
        let ctx = Context::default();
        coursecraft_render::apply_style(&ctx, coursecraft_render::ThemeMode::Dark);
        let screen_rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(w, h));
        let editor = Editor::new(course);
        Self {
            ctx,
            editor,
            screen_rect,
        }
    }

    pub fn frame(&mut self, events: Vec<Event>) -> Vec<EditorRequest> {
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            events,
            ..Default::default()
        };
        let mut requests = Vec::new();
        let output = self.ctx.run_ui(raw_input, |ui| {
            requests = self.editor.show(ui);
        });
        output.drop_without_applying_deltas();
        requests
    }

    /// Run an idle frame and return the centers of every painted text equal to
    /// `label` (searching every layer, including open popups).
    pub fn find_texts(&mut self, label: &str) -> Vec<Pos2> {
        fn walk(shape: &egui::epaint::Shape, label: &str, out: &mut Vec<Pos2>) {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    out.push(text_center(t));
                }
                egui::epaint::Shape::Vec(v) => {
                    for shape in v {
                        walk(shape, label, out);
                    }
                }
                _ => {}
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let mut found = Vec::new();
        for clipped in &output.shapes {
            walk(&clipped.shape, label, &mut found);
        }
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and return the center of the first painted text equal to
    /// `label` (searching every layer, including open popups).
    pub fn find_text(&mut self, label: &str) -> Option<Pos2> {
        fn walk(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(text_center(t)),
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, label)),
                _ => None,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let found = output.shapes.iter().find_map(|cs| walk(&cs.shape, label));
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and return `(center, clip_rect)` of the first painted text equal to
    /// `label`, where `clip_rect` is the clip the text was painted with (so callers can tell
    /// "laid out" from "actually visible").
    pub fn find_text_clipped(&mut self, label: &str) -> Option<(Pos2, Rect)> {
        fn walk(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(text_center(t)),
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, label)),
                _ => None,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let found = output
            .shapes
            .iter()
            .find_map(|cs| walk(&cs.shape, label).map(|p| (p, cs.clip_rect)));
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and return the screen-space center of the last painted text equal to
    /// `label` (useful when a modal dialog or popup overlays background content with the same text).
    pub fn find_text_last(&mut self, label: &str) -> Option<Pos2> {
        fn walk(shape: &egui::epaint::Shape, label: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => Some(text_center(t)),
                egui::epaint::Shape::Vec(v) => v.iter().rev().find_map(|s| walk(s, label)),
                _ => None,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let found = output
            .shapes
            .iter()
            .rev()
            .find_map(|cs| walk(&cs.shape, label));
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and find the center of the first painted text within a specific bounding rect.
    pub fn find_text_in(&mut self, label: &str, in_rect: Rect) -> Option<Pos2> {
        fn walk(shape: &egui::epaint::Shape, label: &str, in_rect: Rect) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text() == label => {
                    let center = text_center(t);
                    if in_rect.contains(center) {
                        Some(center)
                    } else {
                        None
                    }
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, label, in_rect)),
                _ => None,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let found = output
            .shapes
            .iter()
            .find_map(|cs| walk(&cs.shape, label, in_rect));
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and return the center of the first text containing `sub`.
    pub fn find_text_containing(&mut self, sub: &str) -> Option<Pos2> {
        fn walk(shape: &egui::epaint::Shape, sub: &str) -> Option<Pos2> {
            match shape {
                egui::epaint::Shape::Text(t) if t.galley.text().contains(sub) => {
                    Some(text_center(t))
                }
                egui::epaint::Shape::Vec(v) => v.iter().find_map(|s| walk(s, sub)),
                _ => None,
            }
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
        });
        let found = output.shapes.iter().find_map(|cs| walk(&cs.shape, sub));
        output.drop_without_applying_deltas();
        found
    }

    /// Run an idle frame and return the center of the first painted icon matching `icon`.
    pub fn find_icon(&mut self, icon: coursecraft_render::Icon) -> Option<Pos2> {
        let text = icon.codepoint().to_string();
        self.find_text(&text)
    }

    /// Run an idle frame and return the center of the last painted icon matching `icon`.
    pub fn find_icon_last(&mut self, icon: coursecraft_render::Icon) -> Option<Pos2> {
        let text = icon.codepoint().to_string();
        self.find_text_last(&text)
    }

    pub fn idle_frame(&mut self) -> Vec<EditorRequest> {
        self.frame(Vec::new())
    }

    /// Move the pointer to `pos` and run a frame so hover-only UI is laid out.
    pub fn hover_at(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.idle_frame();
    }

    pub fn click_at(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }]);
    }

    pub fn click_at_with(&mut self, pos: Pos2, modifiers: Modifiers) {
        self.frame(vec![Event::PointerMoved(pos)]);
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(modifiers)]);
        }
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        }]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        }]);
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
        }
    }

    pub fn right_click_at(&mut self, pos: Pos2) {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed: true,
            modifiers: Default::default(),
        }]);
        self.frame(vec![Event::PointerButton {
            pos,
            button: PointerButton::Secondary,
            pressed: false,
            modifiers: Default::default(),
        }]);
    }

    pub fn drag_by(&mut self, start: Pos2, delta: egui::Vec2, steps: usize, modifiers: Modifiers) {
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(modifiers)]);
        }
        self.frame(vec![Event::PointerMoved(start)]);
        self.frame(vec![Event::PointerButton {
            pos: start,
            button: PointerButton::Primary,
            pressed: true,
            modifiers,
        }]);
        let steps = steps.max(1);
        for i in 1..=steps {
            let t = i as f32 / steps as f32;
            let current = start + delta * t;
            self.frame(vec![Event::PointerMoved(current)]);
        }
        let end_pos = start + delta;
        self.frame(vec![Event::PointerButton {
            pos: end_pos,
            button: PointerButton::Primary,
            pressed: false,
            modifiers,
        }]);
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
        }
    }

    pub fn press(&mut self, key: Key, modifiers: Modifiers) {
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(modifiers)]);
        }
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }]);
        self.frame(vec![Event::Key {
            key,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers,
        }]);
        if modifiers != Modifiers::NONE {
            self.frame(vec![Event::ModifiersChanged(Modifiers::NONE)]);
        }
    }

    pub fn type_text(&mut self, text: &str) {
        self.frame(vec![Event::Text(text.to_owned())]);
    }

    /// Click a text field / DragValue at `pos`, select all, type `text`, and press Enter.
    pub fn set_field(&mut self, pos: Pos2, text: &str) {
        self.click_at(pos);
        self.frame(vec![
            Event::Key {
                key: Key::A,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            },
            Event::Text(text.to_owned()),
            Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::NONE,
            },
        ]);
        self.idle_frame();
    }

    /// Click a multiline text edit at `pos`, select all, type `text`, and click away to commit on blur.
    pub fn set_multiline_field(&mut self, pos: Pos2, text: &str, click_away_pos: Pos2) {
        self.click_at(pos);
        self.idle_frame();
        self.press(Key::A, Modifiers::COMMAND);
        self.type_text(text);
        self.click_at(click_away_pos);
        self.idle_frame();
    }

    /// Open a combo box showing `combo_label` and select `item_label` in the popup.
    pub fn pick_combo(&mut self, combo_label: &str, item_label: &str) {
        let combo_pos = self
            .find_text_last(combo_label)
            .unwrap_or_else(|| panic!("combo '{combo_label}' not found"));
        self.click_at(combo_pos);
        self.idle_frame();
        let item_pos = self.find_text_last(item_label).unwrap_or_else(|| {
            panic!("item '{item_label}' not found in popup for '{combo_label}'")
        });
        self.click_at(item_pos);
        self.idle_frame();
    }

    /// Deliver a mouse wheel scroll event over `pos`. Negative `dy` scrolls downward.
    pub fn scroll_over(&mut self, pos: Pos2, dy: f32) {
        self.frame(vec![Event::PointerMoved(pos)]);
        self.frame(vec![Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: vec2(0.0, dy),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: Modifiers::NONE,
        }]);
        for _ in 0..8 {
            self.idle_frame();
        }
    }

    /// Top edge of the bottom panel as laid out in the last frame.
    pub fn bottom_panel_top(&self) -> f32 {
        self.editor.ui.timeline.panel_rect.map_or(
            self.screen_rect.max.y - STATUS_BAR_HEIGHT - TIMELINE_PANEL_HEIGHT,
            |r| r.min.y,
        )
    }

    /// Total height of top panels (menu bar + toolbar).
    pub fn top_bars_height(&self) -> f32 {
        TOP_BAR_HEIGHT + crate::toolbar::TOOLBAR_HEIGHT
    }

    pub fn left_panel_rect(&self) -> Rect {
        Rect::from_min_max(
            pos2(0.0, self.top_bars_height()),
            pos2(SLIDES_PANEL_WIDTH, self.bottom_panel_top()),
        )
    }

    pub fn central_panel_rect(&self) -> Rect {
        Rect::from_min_max(
            pos2(SLIDES_PANEL_WIDTH, self.top_bars_height()),
            pos2(
                self.screen_rect.max.x - PROPERTIES_PANEL_WIDTH,
                self.bottom_panel_top(),
            ),
        )
    }

    pub fn right_panel_rect(&self) -> Rect {
        Rect::from_min_max(
            pos2(
                self.screen_rect.max.x - PROPERTIES_PANEL_WIDTH,
                self.top_bars_height(),
            ),
            pos2(self.screen_rect.max.x, self.bottom_panel_top()),
        )
    }
}
