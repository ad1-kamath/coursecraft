//! Shared test harness for `coursecraft-ui-egui` headless UI tests.

use crate::*;
use coursecraft_course::Course;
use egui::epaint::{ClippedShape, Shape};
use egui::{
    Context, Event, Key, LayerId, Modifiers, Order, PointerButton, Pos2, RawInput, Rect, pos2, vec2,
};

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

    pub fn new_with_size(course: Course, size: (f32, f32)) -> Self {
        Self::new_with_theme(course, size, coursecraft_render::ThemeMode::Dark)
    }

    pub fn new_with_theme(
        course: Course,
        (w, h): (f32, f32),
        mode: coursecraft_render::ThemeMode,
    ) -> Self {
        let ctx = Context::default();
        coursecraft_render::apply_style(&ctx, mode);
        // Like the snapshot renderer: no animation, so a window is at its final size and
        // position after the warm-up frames.
        let mut style = (*ctx.global_style()).clone();
        style.animation_time = 0.0;
        ctx.set_global_style(style);
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

/// What one layout-checked frame found.
#[derive(Debug, Default)]
pub(crate) struct LayoutReport {
    /// One readable line per problem.
    pub findings: Vec<String>,
    /// Every non-blank text painted in the frame (to prove a window really was on screen).
    pub texts: Vec<String>,
}

impl LayoutReport {
    pub fn painted(&self, needle: &str) -> bool {
        self.texts.iter().any(|t| t.contains(needle))
    }
}

impl TestHarness {
    /// Run `warmup` idle frames, then one more frame whose painted shapes are checked for
    /// clipped, overlapping and off-window text. See [`check_layout`].
    pub fn layout_report(&mut self, warmup: usize) -> LayoutReport {
        for _ in 0..warmup {
            self.idle_frame();
        }
        let raw_input = RawInput {
            screen_rect: Some(self.screen_rect),
            ..Default::default()
        };
        let screen = self.screen_rect;
        let mut layers: Vec<(LayerId, Vec<ClippedShape>)> = Vec::new();
        let mut regions = Vec::new();
        let output = self.ctx.run_ui(raw_input, |ui| {
            let _ = self.editor.show(ui);
            let ctx = ui.ctx().clone();
            regions = crate::content_region::content_regions(&ctx);
            layers = capture_layers(&ctx);
        });
        let mut report = check_layout(&layers, screen, &regions);
        // The per-layer walk must see every text the final output has, or a layer was missed.
        let from_layers: usize = layers
            .iter()
            .map(|(_, shapes)| shapes.iter().map(|c| count_texts(&c.shape)).sum::<usize>())
            .sum();
        let from_output: usize = output.shapes.iter().map(|c| count_texts(&c.shape)).sum();
        if from_layers != from_output {
            report.findings.push(format!(
                "internal: layer walk saw {from_layers} texts, final output has {from_output}"
            ));
        }
        output.drop_without_applying_deltas();
        report
    }
}

/// Every layer's painted shapes so far this pass. Only valid inside the `run_ui` closure: the
/// final output has already lost the layer ids.
pub(crate) fn capture_layers(ctx: &Context) -> Vec<(LayerId, Vec<ClippedShape>)> {
    let mut ids: Vec<LayerId> = ctx.memory(|m| m.layer_ids().collect());
    ids.push(LayerId::background());
    ids.sort_by_key(|id| (id.order, id.id.value()));
    ids.dedup();
    let mut layers = Vec::new();
    ctx.graphics(|g| {
        for id in ids {
            if let Some(list) = g.get(id) {
                layers.push((id, list.all_entries().cloned().collect()));
            }
        }
    });
    layers
}

fn count_texts(shape: &Shape) -> usize {
    match shape {
        Shape::Text(_) => 1,
        Shape::Vec(v) => v.iter().map(count_texts).sum(),
        _ => 0,
    }
}

struct BoxItem {
    rect: Rect,
    clip: Rect,
}

struct TextItem {
    text: String,
    /// Where the whole galley sits.
    full: Rect,
    /// The part of it a viewer can see.
    visible: Rect,
    clip: Rect,
}

fn is_invisible(t: &egui::epaint::TextShape) -> bool {
    if t.opacity_factor <= 0.0 {
        return true;
    }
    let colors_clear = t
        .galley
        .job
        .sections
        .iter()
        .all(|s| s.format.color == egui::Color32::TRANSPARENT);
    colors_clear && t.override_text_color.is_none_or(|c| c.a() == 0)
}

fn collect_boxes(shape: &Shape, clip: Rect, out: &mut Vec<BoxItem>) {
    match shape {
        Shape::Rect(r) => {
            let drawn = r.fill.a() > 0 || (r.stroke.width > 0.0 && r.stroke.color.a() > 0);
            // Small framed widgets only: panel and card backgrounds are meant to bleed.
            if drawn && r.rect.is_positive() && r.rect.height() < 120.0 && r.rect.width() < 600.0 {
                out.push(BoxItem { rect: r.rect, clip });
            }
        }
        Shape::Vec(v) => {
            for s in v {
                collect_boxes(s, clip, out);
            }
        }
        _ => {}
    }
}

fn collect_texts(shape: &Shape, clip: Rect, out: &mut Vec<TextItem>) {
    match shape {
        Shape::Text(t) => {
            let text = t.galley.text().to_owned();
            if text.trim().is_empty() || is_invisible(t) {
                return;
            }
            let full = t.galley.rect.translate(t.pos.to_vec2());
            let visible = full.intersect(clip);
            if !visible.is_positive() {
                return;
            }
            out.push(TextItem {
                text,
                full,
                visible,
                clip,
            });
        }
        Shape::Vec(v) => {
            for s in v {
                collect_texts(s, clip, out);
            }
        }
        _ => {}
    }
}

fn layer_name(id: LayerId) -> String {
    id.short_debug_format()
}

fn fmt_rect(r: Rect) -> String {
    format!(
        "[{:.0},{:.0} .. {:.0},{:.0}]",
        r.min.x, r.min.y, r.max.x, r.max.y
    )
}

fn shorten(text: &str) -> String {
    let one_line: String = text
        .chars()
        .map(|c| if c == '\n' { ' ' } else { c })
        .collect();
    if one_line.chars().count() > 40 {
        let cut: String = one_line.chars().take(40).collect();
        format!("{cut}...")
    } else {
        one_line
    }
}

/// Look for editor-chrome text that is cut off, runs under other text, or leaves the window.
///
/// `layers` holds every layer's painted shapes (tooltips are skipped). Text whose visible centre
/// is inside a registered slide-content region of its own layer is left alone, because slide
/// content may overlap by design. Reports:
/// - **clipped:** the label extends more than 1 pt past the clip rect it was painted with, left
///   or right, so part of it is cut off;
/// - **overlap:** two labels of one layer whose visible rects intersect by more than 2 pt in
///   both axes;
/// - **off-window:** visible text beyond `screen`.
pub(crate) fn check_layout(
    layers: &[(LayerId, Vec<ClippedShape>)],
    screen: Rect,
    regions: &[(LayerId, Rect)],
) -> LayoutReport {
    const CLIP_TOLERANCE: f32 = 1.0;
    const OVERLAP_TOLERANCE: f32 = 2.0;
    let mut findings = Vec::new();
    let mut texts = Vec::new();
    for (layer, shapes) in layers {
        if layer.order == Order::Tooltip {
            continue;
        }
        let name = layer_name(*layer);
        let mut items = Vec::new();
        for clipped in shapes {
            collect_texts(&clipped.shape, clipped.clip_rect, &mut items);
        }
        let mut boxes = Vec::new();
        for clipped in shapes {
            collect_boxes(&clipped.shape, clipped.clip_rect, &mut boxes);
        }
        for b in &boxes {
            let visible = b.rect.intersect(b.clip);
            if !visible.is_positive()
                || regions
                    .iter()
                    .any(|(l, r)| l == layer && r.contains(visible.center()))
            {
                continue;
            }
            if b.rect.max.x - b.clip.max.x > 1.5 && visible.width() > 4.0 {
                findings.push(format!(
                    "clipped-widget: {name} box {} is cut off at the right by its clip {}",
                    fmt_rect(b.rect),
                    fmt_rect(b.clip)
                ));
            }
        }
        texts.extend(items.iter().map(|item| item.text.clone()));
        items.retain(|item| {
            !regions
                .iter()
                .any(|(l, r)| l == layer && r.contains(item.visible.center()))
        });
        for item in &items {
            let over_right = item.full.max.x - item.clip.max.x;
            let over_left = item.clip.min.x - item.full.min.x;
            if over_right > CLIP_TOLERANCE || over_left > CLIP_TOLERANCE {
                findings.push(format!(
                    "clipped: {name} \"{}\" {} is cut off by its clip {}",
                    shorten(&item.text),
                    fmt_rect(item.full),
                    fmt_rect(item.clip)
                ));
            }
            let v = item.visible;
            if item.full.max.x > screen.max.x + CLIP_TOLERANCE
                || item.full.min.x < screen.min.x - CLIP_TOLERANCE
                || v.max.y > screen.max.y + CLIP_TOLERANCE
                || v.min.y < screen.min.y - CLIP_TOLERANCE
            {
                findings.push(format!(
                    "off-window: {name} \"{}\" {} is outside the window {}",
                    shorten(&item.text),
                    fmt_rect(item.full),
                    fmt_rect(screen)
                ));
            }
        }
        // Sweep by left edge so a busy layer stays cheap.
        let mut order: Vec<usize> = (0..items.len()).collect();
        order.sort_by(|a, b| items[*a].visible.min.x.total_cmp(&items[*b].visible.min.x));
        for (pos, &i) in order.iter().enumerate() {
            let a = &items[i];
            for &j in &order[pos + 1..] {
                let b = &items[j];
                if b.visible.min.x >= a.visible.max.x - OVERLAP_TOLERANCE {
                    break;
                }
                let both = a.visible.intersect(b.visible);
                if both.width() <= OVERLAP_TOLERANCE || both.height() <= OVERLAP_TOLERANCE {
                    continue;
                }
                // The same text drawn twice in one place (a faux-bold or a doubled label) is
                // not something a viewer can see.
                let same_spot = a.text == b.text
                    && (a.visible.min - b.visible.min).length() < 1.0
                    && (a.visible.max - b.visible.max).length() < 1.0;
                if same_spot {
                    continue;
                }
                findings.push(format!(
                    "overlap: {name} \"{}\" {} runs under \"{}\" {}",
                    shorten(&a.text),
                    fmt_rect(a.visible),
                    shorten(&b.text),
                    fmt_rect(b.visible)
                ));
            }
        }
    }
    LayoutReport { findings, texts }
}
