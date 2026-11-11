//! Captions from audio and narration from text (§ M9 step 6).
//!
//! Both windows send one request at a time through the app's `AiClient`. The
//! request and response code lives in `coursecraft-ai`; this module only shows
//! what will be sent and reviews the answer. Nothing changes until the author
//! presses `Use these captions` or `Add to slide`, and each of those is one
//! ordinary engine command, so one undo reverses it.
//!
//! A binary answer never goes through text: speech is kept as bytes until it is
//! stored as an ordinary MP3 in the media library.

use std::sync::Arc;

use coursecraft_ai::{
    AiConfig, AiReply, MAX_AUDIO_UPLOAD, MAX_SPEECH_CHARS, check_audio_config, endpoint_host,
    endpoint_is_local,
};
use coursecraft_media::parse_webvtt;
use coursecraft_playback::{MediaBackend, MediaKind};
use coursecraft_render::{Icon, SIZE_BODY, SIZE_SMALL, palette};
use egui::{Align2, Button, ComboBox, Id, Key, Modifiers, RichText, ScrollArea, TextEdit, Window};

use crate::elevenlabs::{
    SpeechSource, commit_pasted_voice as commit_pasted_elevenlabs_voice,
    push_save as push_elevenlabs_save, request_models as request_elevenlabs_models,
    request_voices as request_elevenlabs_voices,
};
use crate::{AiPurpose, Editor, EditorRequest};

/// Where an audio request has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AudioPhase {
    /// Waiting for the author.
    #[default]
    Idle,
    /// One request is in flight.
    Running,
    /// An answer is ready to review.
    Ready,
}

/// The Captions from Audio window.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CaptionsState {
    /// Whether the window is open.
    pub open: bool,
    /// The slide that owns the object.
    pub slide: String,
    /// The audio or video object.
    pub object: String,
    /// The media key, shown in the consent line and used for the VTT name.
    pub file_name: String,
    /// Size of the file in bytes.
    pub size: usize,
    /// Request state.
    pub phase: AudioPhase,
    /// The answer, once it parsed as timed captions.
    pub vtt: Option<String>,
    /// Number of cues in the answer.
    pub cue_count: usize,
    /// The first few cue texts, for the read-only preview.
    pub preview: Vec<String>,
    /// Whether the object already had captions when the window opened.
    pub had_captions: bool,
    /// A plain error or notice.
    pub error: Option<String>,
}

impl CaptionsState {
    /// True while a request is in flight.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.phase == AudioPhase::Running
    }
}

/// Object id used for the narration audio being previewed in the window.
const PREVIEW_OBJECT: &str = "speech-narration-preview";

/// The Narration from Text window.
#[derive(Default)]
pub struct SpeechState {
    /// Whether the window is open.
    pub open: bool,
    /// The slide the audio will be added to.
    pub slide: String,
    /// The editable text.
    pub text: String,
    /// Request state.
    pub phase: AudioPhase,
    /// The MP3 answer, as raw bytes.
    pub mp3: Option<Vec<u8>>,
    /// A plain error or notice.
    pub error: Option<String>,
    /// Whether the generated audio is playing.
    pub playing: bool,
    /// Playback backend created while the window is open.
    pub player: Option<Box<dyn MediaBackend>>,
}

impl SpeechState {
    /// True while a request is in flight.
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.phase == AudioPhase::Running
    }
}

impl Editor {
    /// Open the caption window for an audio or video object.
    ///
    /// Returns false when the object is not media, its file is missing, or the
    /// file is over the upload limit (the inspector hides the button then).
    pub fn open_captions_dialog(&mut self, slide_id: &str, object_id: &str) -> bool {
        if self.ui.captions.is_running() || self.ui.master_view {
            return false;
        }
        let Some(slide) = self.session.course.slide(slide_id) else {
            return false;
        };
        let Some((object, _)) = slide.object(object_id) else {
            return false;
        };
        let (src, captions) = match &object.kind {
            coursecraft_course::ObjectKind::Audio { src, captions, .. }
            | coursecraft_course::ObjectKind::Video { src, captions, .. } => {
                (src.clone(), captions.clone())
            }
            _ => return false,
        };
        let Some(size) = media_size(&self.media, &src) else {
            return false;
        };
        if size > MAX_AUDIO_UPLOAD {
            return false;
        }
        self.ui.captions = CaptionsState {
            open: true,
            slide: slide_id.to_string(),
            object: object_id.to_string(),
            file_name: src,
            size,
            had_captions: captions.is_some(),
            ..CaptionsState::default()
        };
        true
    }

    /// Open the narration window with an empty field.
    pub fn open_speech_dialog(&mut self) {
        if self.ui.master_view {
            self.set_status("Not available in Master View");
            return;
        }
        self.ui.speech = SpeechState {
            open: true,
            slide: self.selection.slide.clone(),
            ..SpeechState::default()
        };
    }

    /// Open the narration window filled with a slide's notes.
    pub fn open_narration_from_notes(&mut self, slide_id: &str) {
        if self.ui.master_view {
            self.set_status("Not available in Master View");
            return;
        }
        let Some(slide) = self.session.course.slide(slide_id) else {
            return;
        };
        self.ui.speech = SpeechState {
            open: true,
            slide: slide_id.to_string(),
            text: slide.notes.clone(),
            ..SpeechState::default()
        };
    }

    /// Hand a finished captions or speech request to its window.
    pub fn set_audio_result(&mut self, purpose: AiPurpose, result: Result<AiReply, String>) {
        match purpose {
            AiPurpose::Captions { object, slide } => {
                set_captions_result(self, &object, &slide, result);
            }
            AiPurpose::Speech { slide } => set_speech_result(self, &slide, result),
            _ => {}
        }
    }

    /// Forget an in-flight audio request after Cancel.
    pub fn set_audio_cancelled(&mut self, purpose: AiPurpose) {
        match purpose {
            AiPurpose::Captions { object, slide } => {
                let state = &mut self.ui.captions;
                if state.open && state.object == object && state.slide == slide {
                    state.phase = AudioPhase::Idle;
                    state.error = None;
                }
            }
            AiPurpose::Speech { slide } => {
                let state = &mut self.ui.speech;
                if state.open && state.slide == slide {
                    state.phase = AudioPhase::Idle;
                    state.error = None;
                }
            }
            _ => {}
        }
    }

    /// Save a generated VTT in the media store and point the object at it, in
    /// one undo step. Returns the media key.
    pub fn set_generated_captions(
        &mut self,
        slide_id: &str,
        object_id: &str,
        file_name_hint: &str,
        vtt: &str,
    ) -> Option<String> {
        let name = crate::sanitize_media_src(file_name_hint, &self.media);
        self.media.push((name.clone(), Arc::from(vtt.as_bytes())));
        let result = self.run(
            "object.set_captions",
            serde_json::json!({
                "slide": slide_id,
                "object": object_id,
                "captions": name,
            }),
        );
        if result.is_none() {
            self.media.retain(|(key, _)| key != &name);
            return None;
        }
        Some(name)
    }

    /// Save an MP3 in the media store and add it to a slide as an autoplaying
    /// audio object in the bottom-right corner, in one undo step. Returns the
    /// new object id.
    pub fn add_narration_audio(&mut self, slide_id: &str, mp3: &[u8]) -> Option<String> {
        self.add_narration_audio_with_extension(slide_id, mp3, "mp3")
    }

    /// Save an audio file in the media store and add it to a slide as an
    /// autoplaying audio object in the bottom-right corner, in one undo step.
    /// Returns the new object id.
    ///
    /// Narration from text keeps its `narration.mp3` key; a recorded take uses
    /// `narration-N.<extension>` so each take is a file of its own.
    pub fn add_narration_audio_with_extension(
        &mut self,
        slide_id: &str,
        audio: &[u8],
        extension: &str,
    ) -> Option<String> {
        let _ = self.session.course.slide(slide_id)?;
        let name = narration_file_name(extension, &self.media);
        self.media.push((name.clone(), Arc::from(audio)));
        let slide_size = self.session.course.slide_size;
        let width = 48.0_f32;
        let height = 48.0_f32;
        let margin = 16.0_f32;
        let x = (slide_size.w as f32 - width - margin).max(0.0);
        let y = (slide_size.h as f32 - height - margin).max(0.0);
        let result = self.run(
            "object.add",
            serde_json::json!({
                "slide": slide_id,
                "kind": {
                    "type": "audio",
                    "src": name,
                    "autoplay": true,
                },
                "frame": { "x": x, "y": y, "w": width, "h": height },
                "name": "Narration",
            }),
        );
        let Some(value) = result else {
            self.media.retain(|(key, _)| key != &name);
            return None;
        };
        value
            .get("id")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
    }
}

/// Media key for a new narration file. Speech keeps `narration.mp3`; recorded
/// takes are numbered from `narration-1.<extension>`.
fn narration_file_name(extension: &str, existing: &[(String, Arc<[u8]>)]) -> String {
    if extension == "mp3" {
        return crate::sanitize_media_src("narration.mp3", existing);
    }
    let clean_extension: String = extension
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect();
    let extension = if clean_extension.is_empty() {
        "wav"
    } else {
        clean_extension.as_str()
    };
    let mut counter = 1_u32;
    loop {
        let candidate = format!("narration-{counter}.{extension}");
        if !existing.iter().any(|(src, _)| src == &candidate) {
            return candidate;
        }
        counter = counter.saturating_add(1);
    }
}

fn set_captions_result(
    editor: &mut Editor,
    object: &str,
    slide: &str,
    result: Result<AiReply, String>,
) {
    let state = &mut editor.ui.captions;
    if !state.open || state.object != object || state.slide != slide {
        return;
    }
    if state.phase != AudioPhase::Running {
        return;
    }
    state.phase = AudioPhase::Idle;
    match result {
        Ok(AiReply::Captions(vtt)) => match parse_webvtt(vtt.as_bytes()) {
            Ok(cues) if !cues.is_empty() => {
                state.cue_count = cues.len();
                state.preview = cues.iter().take(3).map(|cue| cue.text.clone()).collect();
                state.vtt = Some(vtt);
                state.error = None;
                state.phase = AudioPhase::Ready;
            }
            _ => {
                state.vtt = None;
                state.error = Some("The server didn't return timed captions.".to_string());
            }
        },
        Ok(_) => {
            state.error = Some("The endpoint returned the wrong kind of answer.".to_string());
        }
        Err(message) => state.error = Some(message),
    }
}

fn set_speech_result(editor: &mut Editor, slide: &str, result: Result<AiReply, String>) {
    let state = &mut editor.ui.speech;
    if !state.open || state.slide != slide {
        return;
    }
    if state.phase != AudioPhase::Running {
        return;
    }
    state.phase = AudioPhase::Idle;
    match result {
        Ok(AiReply::Speech(bytes)) if !bytes.is_empty() => {
            state.mp3 = Some(bytes);
            state.error = None;
            state.phase = AudioPhase::Ready;
        }
        Ok(_) => {
            state.mp3 = None;
            state.error = Some("The server didn't return audio.".to_string());
        }
        Err(message) => {
            state.mp3 = None;
            state.error = Some(message);
        }
    }
}

/// Draw the Captions from Audio window when it is open.
pub fn show_captions_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.captions.open {
        return;
    }

    let screen = ctx.content_rect();
    let max_height = (screen.height() - 40.0).max(240.0);
    let config = editor.ui.ai.config.clone();
    let model = config.speech_to_text_model.clone();
    let problem = feature_problem(&config, &model, "Speech-to-text");
    let agent_source = editor.ui.ai.source.is_agent();
    let ready = !agent_source && problem.is_none();

    let mut open = true;
    let mut close_clicked = false;
    let mut generate = false;
    let mut cancel = false;
    let mut use_captions = false;
    let mut open_settings = false;

    let replace = editor.ui.captions.had_captions;

    Window::new("Generate Captions")
        .id(Id::unique(("generate_captions_dialog", 0_u64)))
        .collapsible(false)
        .resizable(true)
        .max_height(max_height)
        .default_width(560.0)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ui.ctx());
            ui.set_max_width(560.0);
            ScrollArea::vertical()
                .id_salt("generate_captions_scroll")
                .max_height((max_height - 60.0).max(180.0))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(
                            "CourseCraft sends the file as it is and turns the answer into \
                             WebVTT captions. Nothing changes until you use them.",
                        )
                        .size(SIZE_SMALL)
                        .color(p.text_secondary),
                    );
                    ui.add_space(6.0);
                    if agent_source {
                        ui.label(
                            RichText::new("Captions need an API endpoint or ElevenLabs")
                                .color(p.text_secondary),
                        );
                        ui.add_space(8.0);
                        if crate::primary_button(ui, "Open AI Settings").clicked() {
                            open_settings = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                        return;
                    }
                    ui.label(
                        RichText::new(consent_line(editor))
                            .size(SIZE_BODY)
                            .color(p.text_secondary),
                    );
                    ui.add_space(6.0);

                    if !ready {
                        ui.label(RichText::new(problem.unwrap_or_default()).color(p.danger));
                        ui.add_space(8.0);
                        if crate::primary_button(ui, "Open AI Settings").clicked() {
                            open_settings = true;
                        }
                        ui.add_space(8.0);
                        if ui.button("Close").clicked() {
                            close_clicked = true;
                        }
                        return;
                    }

                    match editor.ui.captions.phase {
                        AudioPhase::Running => {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new());
                                ui.label("Transcribing…");
                                if ui.button("Cancel").clicked() {
                                    cancel = true;
                                }
                            });
                        }
                        AudioPhase::Ready => {
                            let count = editor.ui.captions.cue_count;
                            let plural = if count == 1 { "cue" } else { "cues" };
                            ui.label(RichText::new(format!("Found {count} {plural}.")).strong());
                            let preview = preview_text(&editor.ui.captions.preview);
                            let mut shown = preview;
                            ui.add(
                                TextEdit::multiline(&mut shown)
                                    .id_salt("captions_preview")
                                    .desired_rows(4)
                                    .interactive(false),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                let label = if replace {
                                    "Replace captions"
                                } else {
                                    "Use these captions"
                                };
                                if crate::primary_button(ui, label).clicked() {
                                    use_captions = true;
                                }
                                if ui.button("Close").clicked() {
                                    close_clicked = true;
                                }
                            });
                        }
                        AudioPhase::Idle => {
                            ui.horizontal(|ui| {
                                if ui.add(Button::new("Generate")).clicked() {
                                    generate = true;
                                }
                                if ui.button("Close").clicked() {
                                    close_clicked = true;
                                }
                            });
                        }
                    }

                    if let Some(error) = &editor.ui.captions.error {
                        ui.add_space(6.0);
                        ui.label(RichText::new(error).color(p.danger));
                    }
                });

            if ui
                .ctx()
                .input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
                && !editor.ui.captions.is_running()
            {
                close_clicked = true;
            }
        });

    if !open {
        close_clicked = true;
    }
    if open_settings {
        if editor.ui.captions.is_running() {
            requests.push(EditorRequest::CancelAi(AiPurpose::Captions {
                object: editor.ui.captions.object.clone(),
                slide: editor.ui.captions.slide.clone(),
            }));
        }
        editor.ui.captions = CaptionsState::default();
        editor.open_ai_settings();
        return;
    }
    if cancel {
        let purpose = AiPurpose::Captions {
            object: editor.ui.captions.object.clone(),
            slide: editor.ui.captions.slide.clone(),
        };
        editor.ui.captions.phase = AudioPhase::Idle;
        requests.push(EditorRequest::CancelAi(purpose));
        return;
    }
    if generate {
        start_captions(editor, requests);
        return;
    }
    if use_captions {
        apply_captions(editor);
        return;
    }
    if close_clicked {
        if editor.ui.captions.is_running() {
            requests.push(EditorRequest::CancelAi(AiPurpose::Captions {
                object: editor.ui.captions.object.clone(),
                slide: editor.ui.captions.slide.clone(),
            }));
        }
        editor.ui.captions = CaptionsState::default();
    }
}

fn start_captions(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    let state = &mut editor.ui.captions;
    state.phase = AudioPhase::Idle;
    state.error = None;
    state.vtt = None;
    let Some(bytes) = media_bytes(&editor.media, &state.file_name) else {
        state.error = Some("The file is missing.".to_string());
        return;
    };
    let config = editor.ui.ai.config.clone();
    let model = config.speech_to_text_model.trim().to_string();
    if let Err(message) = check_audio_config(&config, &model) {
        state.error = Some(message.to_string());
        return;
    }
    let purpose = AiPurpose::Captions {
        object: state.object.clone(),
        slide: state.slide.clone(),
    };
    let file_name = state.file_name.clone();
    state.phase = AudioPhase::Running;
    requests.push(EditorRequest::AiTranscribe {
        purpose,
        model,
        file_name,
        bytes: bytes.to_vec(),
    });
}

fn apply_captions(editor: &mut Editor) {
    let state = editor.ui.captions.clone();
    let Some(vtt) = state.vtt.clone() else {
        return;
    };
    let file_name = captions_name(&state.file_name);
    let Some(saved) = editor.set_generated_captions(&state.slide, &state.object, &file_name, &vtt)
    else {
        editor.ui.captions.error =
            Some("Couldn't save the captions. Nothing was changed.".to_string());
        return;
    };
    editor.set_status(format!("Saved captions as {saved}."));
    editor.ui.captions = CaptionsState::default();
}

/// Draw the Narration from Text window when it is open.
pub fn show_speech_window(
    editor: &mut Editor,
    ctx: &egui::Context,
    requests: &mut Vec<EditorRequest>,
) {
    if !editor.ui.speech.open {
        return;
    }

    let screen = ctx.content_rect();
    let max_height = (screen.height() - 40.0).max(260.0);
    let config = editor.ui.ai.config.clone();
    let model = config.text_to_speech_model.clone();
    let problem = feature_problem(&config, &model, "Text-to-speech");
    let agent_source = editor.ui.ai.source.is_agent();
    let ready = !agent_source && problem.is_none();

    let mut open = true;
    let mut close_clicked = false;
    let mut play = false;
    let mut stop_playback = false;
    let mut generate = false;
    let mut cancel = false;
    let mut add_to_slide = false;
    let mut open_settings = false;
    let mut source_changed = false;
    let mut key_committed = false;
    let mut refresh_voices = false;
    let mut paste_committed = false;
    let mut model_selected: Option<String> = None;
    let mut voice_selected: Option<(String, String)> = None;

    if let Some(player) = editor.ui.speech.player.as_mut()
        && player
            .take_ended()
            .iter()
            .any(|object| object == PREVIEW_OBJECT)
    {
        editor.ui.speech.playing = false;
    }

    Window::new("Narration from Text")
        .id(Id::unique(("narration_from_text_dialog", 0_u64)))
        .collapsible(false)
        .resizable(true)
        .max_height(max_height)
        .default_width(560.0)
        .pivot(Align2::CENTER_CENTER)
        .default_pos(screen.center())
        .open(&mut open)
        .show(ctx, |ui| {
            let p = palette(ui.ctx());
            ui.set_max_width(560.0);
            ScrollArea::vertical()
                .id_salt("narration_from_text_scroll")
                .max_height((max_height - 60.0).max(200.0))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Voice source");
                        for option in SpeechSource::ALL {
                            let selected = editor.ui.speech_source == option;
                            if ui.selectable_label(selected, option.label()).clicked() && !selected
                            {
                                editor.ui.speech_source = option;
                                source_changed = true;
                            }
                        }
                    });
                    ui.add_space(6.0);

                    if editor.ui.speech_source == SpeechSource::AiProvider {
                        if agent_source {
                            ui.label(
                                RichText::new("Narration needs an API endpoint or ElevenLabs")
                                    .color(p.text_secondary),
                            );
                            ui.add_space(8.0);
                            if crate::primary_button(ui, "Open AI Settings").clicked() {
                                open_settings = true;
                            }
                            ui.add_space(8.0);
                            if ui.button("Close").clicked() {
                                close_clicked = true;
                            }
                            return;
                        }
                        if !ready {
                            ui.label(
                                RichText::new(problem.clone().unwrap_or_default()).color(p.danger),
                            );
                            ui.add_space(8.0);
                            if crate::primary_button(ui, "Open AI Settings").clicked() {
                                open_settings = true;
                            }
                            ui.add_space(8.0);
                            if ui.button("Close").clicked() {
                                close_clicked = true;
                            }
                            return;
                        }
                    } else {
                        show_elevenlabs_settings(
                            ui,
                            editor,
                            &mut key_committed,
                            &mut refresh_voices,
                            &mut paste_committed,
                            &mut model_selected,
                            &mut voice_selected,
                        );
                    }

                    let characters = editor.ui.speech.text.chars().count();
                    let limit = if editor.ui.speech_source == SpeechSource::ElevenLabs {
                        editor.ui.elevenlabs.max_chars()
                    } else {
                        MAX_SPEECH_CHARS
                    };
                    ui.add(
                        TextEdit::multiline(&mut editor.ui.speech.text)
                            .id_salt("narration_text")
                            .desired_rows(6)
                            .hint_text("Type the words to speak"),
                    );
                    let count_color = if characters > limit {
                        p.danger
                    } else {
                        p.text_secondary
                    };
                    ui.label(
                        RichText::new(format!("{characters} of {limit} characters"))
                            .size(SIZE_SMALL)
                            .color(count_color),
                    );
                    if has_variable_reference(&editor.ui.speech.text) {
                        ui.label(
                            RichText::new(
                                "%name% references are read aloud literally, because speech is \
                                 made once.",
                            )
                            .size(SIZE_SMALL)
                            .color(p.warning),
                        );
                    }
                    ui.add_space(6.0);
                    let consent = if editor.ui.speech_source == SpeechSource::ElevenLabs {
                        format!(
                            "This sends {characters} characters to ElevenLabs, which uses your \
                             account's credits."
                        )
                    } else {
                        speech_consent_line(editor, characters)
                    };
                    ui.label(
                        RichText::new(consent)
                            .size(SIZE_BODY)
                            .color(p.text_secondary),
                    );
                    ui.label(
                        RichText::new(
                            "Tell your learners this voice is AI-generated, if your provider's \
                             terms require it.",
                        )
                        .size(SIZE_SMALL)
                        .color(p.text_tertiary),
                    );
                    ui.add_space(8.0);

                    match editor.ui.speech.phase {
                        AudioPhase::Running => {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new());
                                ui.label("Making audio…");
                                if ui.button("Cancel").clicked() {
                                    cancel = true;
                                }
                            });
                        }
                        AudioPhase::Ready => {
                            let size = editor.ui.speech.mp3.as_ref().map_or(0, |bytes| bytes.len());
                            ui.label(
                                RichText::new(format!(
                                    "Audio is ready ({:.1} KB).",
                                    size as f64 / 1024.0
                                ))
                                .strong(),
                            );
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if editor.ui.speech.playing {
                                    if ui.button("Stop playback").clicked() {
                                        stop_playback = true;
                                    }
                                } else if ui.button("Play").clicked() {
                                    play = true;
                                }
                                if crate::primary_button(ui, "Add to slide").clicked() {
                                    add_to_slide = true;
                                }
                                if ui.button("Close").clicked() {
                                    close_clicked = true;
                                }
                            });
                        }
                        AudioPhase::Idle => {
                            let elevenlabs_problem =
                                if editor.ui.speech_source == SpeechSource::ElevenLabs {
                                    elevenlabs_generate_problem(editor, characters)
                                } else {
                                    None
                                };
                            let can_generate =
                                if editor.ui.speech_source == SpeechSource::ElevenLabs {
                                    elevenlabs_problem.is_none()
                                } else {
                                    !editor.ui.speech.text.trim().is_empty()
                                        && characters <= MAX_SPEECH_CHARS
                                };
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(can_generate, Button::new("Generate"))
                                    .clicked()
                                {
                                    generate = true;
                                }
                                if ui.button("Close").clicked() {
                                    close_clicked = true;
                                }
                            });
                            if let Some(reason) = elevenlabs_problem {
                                ui.label(
                                    RichText::new(reason)
                                        .size(SIZE_SMALL)
                                        .color(p.text_secondary),
                                );
                            }
                        }
                    }

                    if let Some(error) = &editor.ui.speech.error {
                        ui.add_space(6.0);
                        ui.label(RichText::new(error).color(p.danger));
                    }
                });

            if ui
                .ctx()
                .input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
                && !editor.ui.speech.is_running()
            {
                close_clicked = true;
            }
        });

    if !open {
        close_clicked = true;
    }

    if source_changed {
        push_elevenlabs_save(editor, requests);
    }
    if let Some(id) = model_selected {
        editor.ui.elevenlabs.config.model_id = id;
        editor.ui.elevenlabs.saved = editor.ui.elevenlabs.config.clone();
        let config = editor.ui.elevenlabs.config.clone();
        requests.push(EditorRequest::SaveElevenLabs(Some(config)));
    }
    if let Some((id, name)) = voice_selected {
        editor.ui.elevenlabs.config.voice_id = id.clone();
        editor.ui.elevenlabs.config.voice_name = name;
        editor.ui.elevenlabs.pasted_id = id;
        editor.ui.elevenlabs.paste_error = None;
        editor.ui.elevenlabs.paste_warning = None;
        editor.ui.elevenlabs.saved = editor.ui.elevenlabs.config.clone();
        let config = editor.ui.elevenlabs.config.clone();
        requests.push(EditorRequest::SaveElevenLabs(Some(config)));
    }
    if key_committed {
        push_elevenlabs_save(editor, requests);
        request_elevenlabs_models(editor, requests);
    } else if editor.ui.speech_source == crate::elevenlabs::SpeechSource::ElevenLabs
        && !editor.ui.elevenlabs.config.api_key.trim().is_empty()
        && !editor.ui.elevenlabs.models_loaded
        && !editor.ui.elevenlabs.listing_running
        && !editor.ui.speech.is_running()
    {
        // A key saved in an earlier session: fill the lists once, unprompted.
        // A failed load also sets `models_loaded`, so this never repeats.
        request_elevenlabs_models(editor, requests);
    }
    if refresh_voices {
        if editor.ui.elevenlabs.models_loaded {
            request_elevenlabs_voices(editor, requests);
        } else {
            request_elevenlabs_models(editor, requests);
        }
    }
    if paste_committed {
        let _ = commit_pasted_elevenlabs_voice(editor, requests);
    }
    // A commit already started a call; let the author press Generate again
    // rather than queueing speech behind a list or lookup.
    if key_committed || refresh_voices || paste_committed {
        generate = false;
    }

    if open_settings {
        if editor.ui.speech.is_running() {
            requests.push(EditorRequest::CancelAi(AiPurpose::Speech {
                slide: editor.ui.speech.slide.clone(),
            }));
        }
        close_speech_window(editor);
        editor.open_ai_settings();
        return;
    }
    if cancel {
        let purpose = AiPurpose::Speech {
            slide: editor.ui.speech.slide.clone(),
        };
        editor.ui.speech.phase = AudioPhase::Idle;
        requests.push(EditorRequest::CancelAi(purpose));
        return;
    }
    if generate {
        start_speech(editor, requests);
        return;
    }
    if add_to_slide {
        apply_speech(editor);
        return;
    }
    if !open || close_clicked {
        if editor.ui.speech.is_running() {
            requests.push(EditorRequest::CancelAi(AiPurpose::Speech {
                slide: editor.ui.speech.slide.clone(),
            }));
        }
        close_speech_window(editor);
        return;
    }
    if play {
        play_speech(editor);
    }
    if stop_playback {
        stop_speech_playback(editor);
    }
    if editor.ui.speech.open && editor.ui.speech.playing {
        ctx.request_repaint();
    }
}

fn show_elevenlabs_settings(
    ui: &mut egui::Ui,
    editor: &mut Editor,
    key_committed: &mut bool,
    refresh_voices: &mut bool,
    paste_committed: &mut bool,
    model_selected: &mut Option<String>,
    voice_selected: &mut Option<(String, String)>,
) {
    let p = palette(ui.ctx());
    let state = &mut editor.ui.elevenlabs;

    ui.horizontal(|ui| {
        ui.label("API key");
        let response = ui.add(
            TextEdit::singleline(&mut state.config.api_key)
                .id_salt("elevenlabs_api_key")
                .password(!state.show_key)
                .desired_width(260.0),
        );
        let label = if state.show_key { "Hide" } else { "Show" };
        if ui.button(label).clicked() {
            state.show_key = !state.show_key;
        }
        if response.lost_focus() && state.config.api_key != state.saved.api_key {
            *key_committed = true;
        }
    });
    ui.horizontal(|ui| {
        ui.hyperlink_to("Get a key", "https://elevenlabs.io/app/settings/api-keys");
    });
    if let Some(error) = &state.models_error {
        ui.label(RichText::new(error).color(p.warning));
    }

    ui.horizontal(|ui| {
        ui.label("Model");
        let selected = state.model_label();
        ComboBox::from_id_salt("elevenlabs_model")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for model in &state.models {
                    let is_selected = model.id == state.model_id();
                    if ui.selectable_label(is_selected, &model.name).clicked() && !is_selected {
                        *model_selected = Some(model.id.clone());
                    }
                }
            });
    });

    let voice_count = state.voices.len();
    if voice_count > 12 {
        ui.horizontal(|ui| {
            ui.label("Filter");
            ui.add(
                TextEdit::singleline(&mut state.voice_filter)
                    .id_salt("elevenlabs_voice_filter")
                    .hint_text("Filter voices")
                    .desired_width(220.0),
            );
        });
    }
    let voices: Vec<(String, String, String)> = state
        .sorted_voices()
        .into_iter()
        .map(|voice| (voice.id.clone(), voice.name.clone(), voice.category.clone()))
        .collect();
    ui.horizontal(|ui| {
        ui.label("Voice");
        let selected = state.voice_label();
        ComboBox::from_id_salt("elevenlabs_voice")
            .selected_text(selected)
            .show_ui(ui, |ui| {
                for (id, name, category) in &voices {
                    let label = if category.is_empty() {
                        name.clone()
                    } else {
                        format!("{name} · {category}")
                    };
                    let is_selected = *id == state.config.voice_id;
                    if ui.selectable_label(is_selected, label).clicked() && !is_selected {
                        *voice_selected = Some((id.clone(), name.clone()));
                    }
                }
            });
        if crate::widgets::icon_button(ui, Icon::ArrowClockwise, "Refresh voices").clicked() {
            *refresh_voices = true;
        }
    });
    if state.voices_loaded && state.voices.is_empty() {
        ui.label(
            RichText::new("No voices came back. Paste a voice ID instead.")
                .size(SIZE_SMALL)
                .color(p.text_secondary),
        );
    }
    if let Some(error) = &state.voices_error {
        ui.label(RichText::new(error).color(p.danger));
    }

    ui.horizontal(|ui| {
        ui.label("Or paste a voice ID");
        let response = ui.add(
            TextEdit::singleline(&mut state.pasted_id)
                .id_salt("elevenlabs_pasted_voice")
                .hint_text("From your ElevenLabs voice library")
                .desired_width(300.0),
        );
        if response.lost_focus()
            && !state.pasted_id.trim().is_empty()
            && state.pasted_id.trim() != state.config.voice_id
        {
            *paste_committed = true;
        }
    });
    if let Some(warning) = &state.paste_warning {
        ui.label(RichText::new(warning).color(p.warning));
    }
    if let Some(error) = &state.paste_error {
        ui.label(RichText::new(error).color(p.danger));
    }
    ui.add_space(8.0);
}

fn elevenlabs_generate_problem(editor: &Editor, characters: usize) -> Option<String> {
    let state = &editor.ui.elevenlabs;
    if state.config.api_key.trim().is_empty() {
        return Some("Enter your ElevenLabs API key.".to_string());
    }
    if let Err(error) = coursecraft_ai::check_voice_id(&state.config.voice_id) {
        return Some(error.to_string());
    }
    if editor.ui.speech.text.trim().is_empty() {
        return Some("Type the words to speak.".to_string());
    }
    let limit = state.max_chars();
    if characters > limit {
        return Some(format!(
            "That's too much text for this model ({characters} of {limit} characters)."
        ));
    }
    None
}

fn start_speech(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    stop_speech_playback(editor);
    editor.ui.speech.phase = AudioPhase::Idle;
    editor.ui.speech.error = None;
    editor.ui.speech.mp3 = None;
    if editor.ui.speech_source == SpeechSource::ElevenLabs {
        start_elevenlabs_speech(editor, requests);
        return;
    }
    let config = editor.ui.ai.config.clone();
    let model = config.text_to_speech_model.trim().to_string();
    if let Err(message) = check_audio_config(&config, &model) {
        editor.ui.speech.error = Some(message.to_string());
        return;
    }
    let text = editor.ui.speech.text.clone();
    let purpose = AiPurpose::Speech {
        slide: editor.ui.speech.slide.clone(),
    };
    let voice = config.voice.clone();
    editor.ui.speech.phase = AudioPhase::Running;
    requests.push(EditorRequest::AiSpeech {
        purpose,
        model,
        voice,
        text,
    });
}

fn start_elevenlabs_speech(editor: &mut Editor, requests: &mut Vec<EditorRequest>) {
    stop_speech_playback(editor);
    let characters = editor.ui.speech.text.chars().count();
    if let Some(problem) = elevenlabs_generate_problem(editor, characters) {
        editor.ui.speech.error = Some(problem);
        return;
    }
    let text = editor.ui.speech.text.clone();
    let purpose = AiPurpose::Speech {
        slide: editor.ui.speech.slide.clone(),
    };
    editor.ui.speech.phase = AudioPhase::Running;
    requests.push(EditorRequest::ElevenLabsSpeech { purpose, text });
}

fn play_speech(editor: &mut Editor) {
    let state = &mut editor.ui.speech;
    let Some(mp3) = state.mp3.clone() else {
        return;
    };
    let player = state
        .player
        .get_or_insert_with(coursecraft_playback::create_default_backend);
    player.play(
        PREVIEW_OBJECT,
        "narration-preview.mp3",
        MediaKind::Audio,
        Arc::from(mp3),
    );
    state.playing = true;
}

fn stop_speech_playback(editor: &mut Editor) {
    let state = &mut editor.ui.speech;
    if let Some(player) = state.player.as_mut() {
        player.stop_all();
    }
    state.playing = false;
}

fn close_speech_window(editor: &mut Editor) {
    stop_speech_playback(editor);
    editor.ui.speech = SpeechState::default();
}

fn apply_speech(editor: &mut Editor) {
    stop_speech_playback(editor);
    let slide = editor.ui.speech.slide.clone();
    let Some(mp3) = editor.ui.speech.mp3.clone() else {
        return;
    };
    if editor.add_narration_audio(&slide, &mp3).is_none() {
        editor.ui.speech.error = Some("Couldn't add the audio. Nothing was changed.".to_string());
        return;
    }
    editor.set_status("Added the narration to the slide.");
    editor.ui.speech = SpeechState::default();
}

fn feature_problem(config: &AiConfig, model: &str, label: &str) -> Option<String> {
    if model.trim().is_empty() {
        return Some(format!("{label} model isn't set."));
    }
    check_audio_config(config, model)
        .err()
        .map(|error| error.to_string())
}

fn consent_line(editor: &Editor) -> String {
    let state = &editor.ui.captions;
    let size = format!("{:.1} MB", state.size as f64 / (1024.0 * 1024.0));
    let host = endpoint_host(&editor.ui.ai.config);
    let local = endpoint_is_local(&editor.ui.ai.config);
    match host {
        Some(host) if local => format!(
            "This sends {} ({size}) to {host}, on this computer.",
            state.file_name
        ),
        Some(host) => format!("This sends {} ({size}) to {host}.", state.file_name),
        None => format!(
            "This sends {} ({size}) to the AI endpoint.",
            state.file_name
        ),
    }
}

fn speech_consent_line(editor: &Editor, characters: usize) -> String {
    let host = endpoint_host(&editor.ui.ai.config);
    let local = endpoint_is_local(&editor.ui.ai.config);
    match host {
        Some(host) if local => {
            format!("This sends {characters} characters of text to {host}, on this computer.")
        }
        Some(host) => format!("This sends {characters} characters of text to {host}."),
        None => format!("This sends {characters} characters of text to the AI endpoint."),
    }
}

fn preview_text(cues: &[String]) -> String {
    cues.join("\n")
}

/// True when the text contains a `%name%` reference, which speech reads out.
fn has_variable_reference(text: &str) -> bool {
    let mut rest = text;
    while let Some(start) = rest.find('%') {
        let after = rest.get(start + 1..).unwrap_or_default();
        let Some(end) = after.find('%') else {
            return false;
        };
        let name = after.get(..end).unwrap_or_default();
        if !name.is_empty() && !name.chars().any(char::is_whitespace) && !name.contains('%') {
            return true;
        }
        rest = after.get(end + 1..).unwrap_or_default();
    }
    false
}

fn captions_name(file_name: &str) -> String {
    let stem = std::path::Path::new(file_name)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .filter(|stem| !stem.trim().is_empty())
        .unwrap_or("captions");
    format!("{stem}.vtt")
}

fn media_size(media: &[(String, Arc<[u8]>)], src: &str) -> Option<usize> {
    media
        .iter()
        .find(|(name, _)| name == src)
        .map(|(_, bytes)| bytes.len())
}

fn media_bytes<'a>(media: &'a [(String, Arc<[u8]>)], src: &str) -> Option<&'a [u8]> {
    media
        .iter()
        .find(|(name, _)| name == src)
        .map(|(_, bytes)| bytes.as_ref())
}
