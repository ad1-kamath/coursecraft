//! Record Narration window (§ M14 step 2).
//!
//! The desktop editor records from the microphone through
//! `coursecraft_playback::record`, previews the take through the ordinary media
//! backend, and adds it as one engine command. The web editor shows the window
//! but cannot record. Everything that touches the microphone is compiled out on
//! wasm.

use crate::dialog::Dialog;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Arc;

#[cfg(not(target_arch = "wasm32"))]
use coursecraft_render::palette;
#[cfg(not(target_arch = "wasm32"))]
use egui::{Button, Color32, ComboBox, CornerRadius, Rect, RichText, Sense, vec2};
use egui::{Id, Key, Modifiers};

#[cfg(not(target_arch = "wasm32"))]
use coursecraft_playback::record::{
    self, RecordError, RecorderBackend, Recording, downmix_to_mono, encode_wav,
};
#[cfg(not(target_arch = "wasm32"))]
use coursecraft_playback::{MediaBackend, MediaKind};

use crate::Editor;

/// Object id used for the take being previewed in the window.
#[cfg(not(target_arch = "wasm32"))]
const PREVIEW_OBJECT: &str = "record-narration-preview";

/// The Record Narration window.
#[derive(Default)]
pub struct RecordNarrationState {
    /// Whether the window is open.
    pub open: bool,
    /// Microphone names from `input_devices()`.
    pub devices: Vec<String>,
    /// The chosen device, or `None` for the system default.
    pub device: Option<String>,
    /// Live recorder while a take is in progress.
    #[cfg(not(target_arch = "wasm32"))]
    pub recorder: Option<Box<dyn RecorderBackend>>,
    /// The finished take, before it is added to the slide.
    #[cfg(not(target_arch = "wasm32"))]
    pub take: Option<Recording>,
    /// The take encoded as a mono WAV, for the size line and playback.
    #[cfg(not(target_arch = "wasm32"))]
    pub wav: Option<Vec<u8>>,
    /// A plain error or notice.
    pub error: Option<String>,
    /// Whether the take is playing.
    pub playing: bool,
    /// Playback backend created while the window is open.
    #[cfg(not(target_arch = "wasm32"))]
    pub player: Option<Box<dyn MediaBackend>>,
}

impl Editor {
    /// Open the Record Narration window and refresh the microphone list.
    pub fn open_record_narration(&mut self) {
        if self.ui.master_view {
            self.set_status("Not available in Master View");
            return;
        }
        close_record_narration(self);
        self.ui.record_narration = RecordNarrationState {
            open: true,
            ..RecordNarrationState::default()
        };
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.ui.record_narration.devices = record::input_devices();
        }
    }
}

/// Start recording with a backend. Tests inject a fake through this seam.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn start_with(editor: &mut Editor, backend: Box<dyn RecorderBackend>) {
    apply_start(editor, Ok(backend));
}

/// Show the error a failed start would show. Tests use this for `NoDevice`.
#[cfg(all(test, not(target_arch = "wasm32")))]
pub(crate) fn start_with_error(editor: &mut Editor, error: RecordError) {
    apply_start(editor, Err(error));
}

#[cfg(not(target_arch = "wasm32"))]
fn apply_start(editor: &mut Editor, result: Result<Box<dyn RecorderBackend>, RecordError>) {
    let state = &mut editor.ui.record_narration;
    state.error = None;
    state.take = None;
    state.wav = None;
    state.playing = false;
    if let Some(mut player) = state.player.take() {
        player.stop_all();
    }
    match result {
        Ok(backend) => state.recorder = Some(backend),
        Err(error) => state.error = Some(start_error_message(&error)),
    }
}

/// The line shown when the microphone cannot start.
#[cfg(not(target_arch = "wasm32"))]
fn start_error_message(error: &RecordError) -> String {
    match error {
        RecordError::NoDevice => "No microphone found".to_owned(),
        other => other.to_string(),
    }
}

/// Close the window, ending any capture and playback.
pub fn close_record_narration(editor: &mut Editor) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if let Some(recorder) = editor.ui.record_narration.recorder.take() {
            let _ = recorder.stop();
        }
        if let Some(mut player) = editor.ui.record_narration.player.take() {
            player.stop_all();
        }
        editor.ui.record_narration.take = None;
        editor.ui.record_narration.wav = None;
    }
    editor.ui.record_narration.open = false;
    editor.ui.record_narration.playing = false;
}

/// Escape closes the window before the canvas sees the key.
pub fn consume_escape(editor: &mut Editor, ctx: &egui::Context) {
    if editor.ui.record_narration.open
        && ctx.input_mut(|input| input.consume_key(Modifiers::NONE, Key::Escape))
    {
        close_record_narration(editor);
    }
}

/// Draw the Record Narration window when it is open.
pub fn show_record_narration_window(editor: &mut Editor, ctx: &egui::Context) {
    if !editor.ui.record_narration.open {
        return;
    }

    let mut open = true;
    let mut close = false;
    Dialog::new("Record Narration")
        .id(Id::unique(("record_narration_dialog", 0_u64)))
        .width(420.0)
        .open(&mut open)
        .show(ctx, |ui| {
            #[cfg(not(target_arch = "wasm32"))]
            {
                close = desktop_contents(editor, ui);
            }
            #[cfg(target_arch = "wasm32")]
            {
                ui.label("Recording works in the desktop app.");
                ui.add_space(8.0);
                if ui.button("Close").clicked() {
                    close = true;
                }
            }
        });

    if !open || close {
        close_record_narration(editor);
    }

    #[cfg(not(target_arch = "wasm32"))]
    if editor.ui.record_narration.open
        && (editor.ui.record_narration.recorder.is_some() || editor.ui.record_narration.playing)
    {
        ctx.request_repaint();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn desktop_contents(editor: &mut Editor, ui: &mut egui::Ui) -> bool {
    let p = palette(ui.ctx());
    let mut start = false;
    let mut stop = false;
    let mut play = false;
    let mut stop_playback = false;
    let mut rerecord = false;
    let mut use_recording = false;
    let mut close = false;
    let mut choose_device: Option<Option<String>> = None;

    {
        let state = &mut editor.ui.record_narration;

        if let Some(player) = state.player.as_mut()
            && player
                .take_ended()
                .iter()
                .any(|object| object == PREVIEW_OBJECT)
        {
            state.playing = false;
        }

        crate::dialog::Form::new(ui, &["Microphone"]).row(ui, "Microphone", |ui, _| {
            let selected = state.device.as_deref().unwrap_or("Default").to_owned();
            ComboBox::from_id_salt("record_narration_device")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(state.device.is_none(), "Default")
                        .clicked()
                    {
                        choose_device = Some(None);
                    }
                    for name in &state.devices {
                        if ui
                            .selectable_label(state.device.as_deref() == Some(name.as_str()), name)
                            .clicked()
                        {
                            choose_device = Some(Some(name.clone()));
                        }
                    }
                });
        });
        ui.add_space(6.0);

        if state.recorder.is_some() {
            if let Some(recorder) = state.recorder.as_ref() {
                level_meter(ui, recorder.level(), &p);
                ui.label(RichText::new(mmss(recorder.elapsed_ms())).monospace());
            }
            ui.add_space(4.0);
            if ui.button("Stop").clicked() {
                stop = true;
            }
        } else if let Some(take) = &state.take {
            ui.label(RichText::new(format!("Take: {}", mmss(recording_ms(take)))).strong());
            let size = state.wav.as_ref().map_or(0, Vec::len);
            ui.label(
                RichText::new(format!("{:.1} KB", size as f64 / 1024.0))
                    .size(coursecraft_render::SIZE_SMALL)
                    .color(p.text_secondary),
            );
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if state.playing {
                    if ui.button("Stop playback").clicked() {
                        stop_playback = true;
                    }
                } else if ui.button("Play").clicked() {
                    play = true;
                }
                if ui.button("Re-record").clicked() {
                    rerecord = true;
                }
                if crate::primary_button(ui, "Use recording").clicked() {
                    use_recording = true;
                }
            });
        } else {
            ui.label(
                RichText::new("Records from the microphone into this slide as narration.")
                    .color(p.text_secondary),
            );
            ui.add_space(6.0);
            let record = Button::new(RichText::new("Record").color(Color32::WHITE)).fill(p.danger);
            if ui.add(record).clicked() {
                start = true;
            }
        }

        if let Some(error) = &state.error {
            ui.add_space(6.0);
            ui.colored_label(p.danger, error);
        }
    }

    if let Some(device) = choose_device {
        editor.ui.record_narration.device = device;
    }
    if start {
        let device = editor.ui.record_narration.device.clone();
        let result = record::Recorder::start(device.as_deref());
        apply_start(editor, result);
    }
    if stop {
        stop_take(editor);
    }
    if play {
        play_take(editor);
    }
    if stop_playback {
        stop_take_playback(editor);
    }
    if rerecord {
        let state = &mut editor.ui.record_narration;
        state.take = None;
        state.wav = None;
        state.error = None;
        state.playing = false;
    }
    if use_recording {
        close = use_take(editor);
    }

    close
}

#[cfg(not(target_arch = "wasm32"))]
fn stop_take(editor: &mut Editor) {
    let Some(recorder) = editor.ui.record_narration.recorder.take() else {
        return;
    };
    match recorder.stop() {
        Ok(recording) => {
            let wav = encode_wav(&downmix_to_mono(&recording));
            let state = &mut editor.ui.record_narration;
            state.take = Some(recording);
            state.wav = Some(wav);
            state.error = None;
            state.playing = false;
        }
        Err(error) => {
            let state = &mut editor.ui.record_narration;
            state.take = None;
            state.wav = None;
            state.error = Some(start_error_message(&error));
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn play_take(editor: &mut Editor) {
    let state = &mut editor.ui.record_narration;
    let Some(wav) = state.wav.clone() else {
        return;
    };
    let player = state
        .player
        .get_or_insert_with(coursecraft_playback::create_default_backend);
    player.play(
        PREVIEW_OBJECT,
        "narration-preview.wav",
        MediaKind::Audio,
        Arc::from(wav),
    );
    state.playing = true;
}

#[cfg(not(target_arch = "wasm32"))]
fn stop_take_playback(editor: &mut Editor) {
    let state = &mut editor.ui.record_narration;
    if let Some(player) = state.player.as_mut() {
        player.stop_all();
    }
    state.playing = false;
}

#[cfg(not(target_arch = "wasm32"))]
fn use_take(editor: &mut Editor) -> bool {
    if editor.ui.master_view {
        editor.set_status("Not available in Master View");
        return false;
    }
    let slide = editor.selection.slide.clone();
    if slide.is_empty() || editor.session.course.slide(&slide).is_none() {
        editor.set_status("Select a slide first");
        return false;
    }
    let Some(take) = editor.ui.record_narration.take.clone() else {
        return false;
    };
    let wav = encode_wav(&downmix_to_mono(&take));
    if wav.is_empty() {
        editor.ui.record_narration.error = Some("Could not encode the recording.".to_owned());
        return false;
    }
    if editor
        .add_narration_audio_with_extension(&slide, &wav, "wav")
        .is_none()
    {
        editor.ui.record_narration.error =
            Some("Could not add the recording to the slide.".to_owned());
        return false;
    }
    editor.set_status("Narration added");
    true
}

#[cfg(not(target_arch = "wasm32"))]
fn level_meter(ui: &mut egui::Ui, level: f32, p: &coursecraft_render::Palette) {
    let level = if level.is_finite() {
        level.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (rect, _) = ui.allocate_exact_size(vec2(240.0, 10.0), Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(2), p.input);
    let width = rect.width() * level;
    if width > 0.0 {
        let fill = Rect::from_min_size(rect.min, vec2(width, rect.height()));
        let colour = if level > 0.9 { p.danger } else { p.accent };
        painter.rect_filled(fill, CornerRadius::same(2), colour);
    }
}

/// Format milliseconds as `m:ss`.
#[cfg(not(target_arch = "wasm32"))]
fn mmss(ms: u64) -> String {
    let total_seconds = ms / 1000;
    format!("{}:{:02}", total_seconds / 60, total_seconds % 60)
}

/// Length of a take in milliseconds.
#[cfg(not(target_arch = "wasm32"))]
fn recording_ms(take: &Recording) -> u64 {
    let per_second = u64::from(take.sample_rate).saturating_mul(u64::from(take.channels));
    if per_second == 0 {
        return 0;
    }
    u64::try_from(take.samples.len())
        .unwrap_or(u64::MAX)
        .saturating_mul(1000)
        / per_second
}
