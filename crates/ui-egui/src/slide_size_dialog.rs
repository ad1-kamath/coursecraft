//! File > Slide Size: pick a preset or custom size and how content follows.

use crate::dialog::Dialog;
use egui::DragValue;
use serde_json::json;

use crate::Editor;

/// Slide size presets: label, width, height.
pub const PRESETS: [(&str, u32, u32); 4] = [
    ("16:9 (1280 x 720)", 1280, 720),
    ("16:10 (1280 x 800)", 1280, 800),
    ("4:3 (960 x 720)", 960, 720),
    ("1:1 (1080 x 1080)", 1080, 1080),
];

/// Dialog state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SlideSizeState {
    pub open: bool,
    pub w: u32,
    pub h: u32,
    /// Scale the content with the slide (true) or keep it as is.
    pub scale: bool,
}

impl Default for SlideSizeState {
    fn default() -> Self {
        Self {
            open: false,
            w: 1280,
            h: 720,
            scale: true,
        }
    }
}

impl SlideSizeState {
    /// Whether the entered size is inside the allowed range.
    #[must_use]
    pub fn valid(&self) -> bool {
        (100..=7680).contains(&self.w) && (100..=4320).contains(&self.h)
    }

    #[must_use]
    pub fn preset_index(&self) -> Option<usize> {
        PRESETS
            .iter()
            .position(|(_, w, h)| *w == self.w && *h == self.h)
    }
}

impl Editor {
    /// Open the dialog on the course's current size.
    pub fn open_slide_size_dialog(&mut self) {
        let size = self.session.course.slide_size;
        self.ui.slide_size_dialog = SlideSizeState {
            open: true,
            w: size.w,
            h: size.h,
            scale: true,
        };
    }
}

/// Show the dialog when open.
pub fn show_slide_size_dialog(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.slide_size_dialog.open {
        return;
    }
    let mut apply = false;
    let mut cancel = false;
    Dialog::new("Slide Size").show(ctx, |ui| {
        let state = &mut editor.ui.slide_size_dialog;
        ui.label("Size");
        for (idx, (label, w, h)) in PRESETS.iter().enumerate() {
            if ui
                .radio(state.preset_index() == Some(idx), *label)
                .clicked()
            {
                state.w = *w;
                state.h = *h;
            }
        }
        let custom = state.preset_index().is_none();
        if ui.radio(custom, "Custom").clicked() && !custom {
            state.w = state.w.saturating_add(1).min(7680);
        }
        ui.horizontal(|ui| {
            ui.label("W");
            ui.add(DragValue::new(&mut state.w).range(0..=100_000));
            ui.label("H");
            ui.add(DragValue::new(&mut state.h).range(0..=100_000));
        });
        if !state.valid() {
            ui.colored_label(
                coursecraft_render::palette(ui.ctx()).danger,
                "Width 100 to 7680, height 100 to 4320.",
            );
        }
        ui.separator();
        ui.radio_value(&mut state.scale, true, "Scale content");
        ui.radio_value(&mut state.scale, false, "Keep content as is");
        ui.separator();
        ui.horizontal(|ui| {
            if ui.button("Cancel").clicked() {
                cancel = true;
            }
            if ui
                .add_enabled(state.valid(), egui::Button::new("Apply"))
                .clicked()
            {
                apply = true;
            }
        });
    });
    if apply {
        let state = editor.ui.slide_size_dialog.clone();
        editor.run(
            "course.set_slide_size",
            json!({
                "w": state.w,
                "h": state.h,
                "mode": if state.scale { "scale" } else { "keep" },
            }),
        );
        editor.ui.slide_size_dialog.open = false;
    }
    if cancel {
        editor.ui.slide_size_dialog.open = false;
    }
}
