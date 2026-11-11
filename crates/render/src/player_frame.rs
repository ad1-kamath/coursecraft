//! Learner-facing player chrome shared by the published player and the editor preview.
//!
//! The frame paints a letterboxed slide on the neutral player ground, a floating
//! control strip and an outline drawer. It only emits the transport and
//! navigation inputs the runtime already understands, and only while the course
//! has started and the course's `player` flags allow it.

use coursecraft_course::fill;
use coursecraft_logic::{Input, Player};
use egui::{
    Align2, Color32, CornerRadius, Event, FontFamily, FontId, Id, Pos2, Rect, RichText, ScrollArea,
    Sense, Stroke, StrokeKind, TextureHandle, Ui, UiBuilder, Vec2, vec2,
};

use crate::chrome::{
    DEFAULT_BAR_COLOR, DEFAULT_BAR_TEXT_COLOR, at_slide_end, format_ms, nav_limits,
    play_pause_inputs, progress_fraction, replay_inputs,
};
use crate::color::parse_color;
use crate::design::{
    ICON_TOOLBAR, PLAYER_GROUND, RADIUS_XL, SIZE_BODY, SIZE_HEADING, SIZE_MONO, SIZE_SMALL,
    SIZE_TITLE, ThemeMode, palette, popover_shadow,
};
use crate::drawer::{DrawerTab, DrawerView, available_tabs, visible_glossary};
use crate::focus::ChromeVisibility;
use crate::icons::Icon;
use crate::media::{MediaCache, MediaSource};
use crate::view::SlideView;

/// Height of the floating control strip.
pub const STRIP_HEIGHT: f32 = 44.0;
/// Distance between the strip and the bottom edge.
pub const STRIP_MARGIN: f32 = 12.0;
/// Width of the outline drawer.
pub const DRAWER_WIDTH: f32 = 320.0;

const STRIP_RADIUS: u8 = 12;
const STRIP_PAD: f32 = 8.0;
const STRIP_BUTTON: f32 = 36.0;
const STRIP_GAP: f32 = 4.0;
const SPEED_BUTTON: f32 = 44.0;
const SPEED_ROW: f32 = 30.0;
const TITLE_MAX: f32 = 280.0;
const SEEK_MIN: f32 = 32.0;
const SEEK_TRACK: f32 = 4.0;
const SEEK_HANDLE: f32 = 6.0;
const SEEK_HANDLE_HOVER: f32 = 8.0;
const VOLUME_SLIDER_WIDTH: f32 = 72.0;
const VOLUME_TRACK: f32 = 3.0;
const VOLUME_HANDLE: f32 = 5.0;
const VOLUME_HANDLE_HOVER: f32 = 6.5;
const AUTO_HIDE_SECONDS: f64 = 2.5;
const FADE_SECONDS: f32 = 0.18;
const DRAWER_SECONDS: f32 = 0.26;

const DRAWER_CLOSE: f32 = 32.0;
const DRAWER_PAD: f32 = 16.0;
const ENDED_GAP: f32 = 8.0;
const ENDED_ICON: f32 = 32.0;
const ENDED_PAD: f32 = 24.0;
const ENDED_CARD_MIN_W: f32 = 280.0;
const ENDED_CARD_FILL: Color32 = Color32::from_rgb(0x23, 0x23, 0x26);

const DRAWER_BG: Color32 = Color32::from_rgb(0x2F, 0x2F, 0x33);
const DRAWER_TEXT: Color32 = Color32::from_rgb(0xF2, 0xF2, 0xF5);
const DRAWER_SECONDARY: Color32 = Color32::from_rgb(0xA8, 0xA8, 0xB0);
const DRAWER_TERTIARY: Color32 = Color32::from_rgb(0x8E, 0x8E, 0x96);

const NOTICE_PAD: f32 = 8.0;
const NOTICE_GAP: f32 = 8.0;
const NOTICE_BUTTON: f32 = 28.0;
const NOTICE_ACCENT: f32 = 3.0;

/// Inputs and layout produced by one player frame.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameOutput {
    /// Runtime inputs from slide interaction, the strip, and the drawer.
    pub inputs: Vec<Input>,
    /// Rect the slide was actually painted into (letterboxed).
    pub slide_rect: Rect,
    /// A new player volume (`0.0`–`1.0`) for the host to apply through
    /// `MediaDriver::set_volume`. `None` when it did not change this frame.
    pub volume: Option<f32>,
    /// The learner asked to toggle full screen. The host acts on it.
    pub fullscreen_toggle: bool,
}

impl Default for FrameOutput {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            slide_rect: Rect::NOTHING,
            volume: None,
            fullscreen_toggle: false,
        }
    }
}

/// Presentational state of the learner player chrome.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerFrame {
    /// Respect the platform's reduced-motion preference: no fades, always shown.
    pub reduced_motion: bool,
    /// The course has ended: dim the slide, show the completion message and
    /// hide the control strip.
    pub ended: bool,
    /// Show captions. A chrome preference, not runtime state; it persists across
    /// slides for the session and defaults to off.
    pub captions_on: bool,
    /// Player volume for the session, `0.0`–`1.0`, default 100%.
    volume: f32,
    /// The last audible level, so unmuting restores it.
    last_volume: f32,
    /// The volume changed this frame and must be pushed to the media driver.
    volume_changed: bool,
    /// The seek value held while the learner drags or clicks the bar.
    seek_drag: Option<u32>,
    /// The slide the current seek drag belongs to.
    seek_drag_slide: Option<String>,
    /// A learner-facing notice, drawn as a slim banner above the slide while it
    /// is set. The app sets it from the bridge's `reporting_lost()` with text
    /// from `course.chrome_string`; the learner can dismiss it for the session.
    pub notice: Option<String>,
    /// The host can change media playback rate (`MediaDriver::supports_rate`).
    /// The speed control is hidden unless this is true.
    pub can_speed: bool,
    /// The host can toggle full screen. The control is hidden unless true.
    pub can_fullscreen: bool,
    /// The host is currently full screen, for the button's look and label.
    pub fullscreen: bool,
    speed_open: bool,
    fullscreen_toggle: bool,
    /// Where the player logo was drawn last frame, if it was.
    last_logo: Option<Rect>,
    /// The drawer's tab, glossary search and open term.
    drawer_view: DrawerView,
    /// Move egui focus into the glossary search field on the next draw.
    focus_glossary_search: bool,
    caption: Option<crate::media_driver::CaptionFrame>,
    last_activity: f64,
    last_time: f64,
    initialized: bool,
    strip_opacity: f32,
    drawer_open: bool,
    drawer_progress: f32,
    last_strip_rect: Option<Rect>,
    last_drawer_rect: Option<Rect>,
    last_ended_card: Option<Rect>,
    last_ended_heading: Option<Rect>,
    last_ended_small: Option<Rect>,
    last_notice_rect: Option<Rect>,
    last_review_rect: Option<Rect>,
    /// The learner dismissed the notice, so it stays hidden for the session.
    notice_dismissed: bool,
    /// Keyboard focus order, ring and keyboard-only gestures.
    focus: crate::focus::FocusManager,
    /// Screen rects of the chrome buttons painted this frame, for the DOM layer.
    last_chrome: Vec<(crate::focus::ChromeButton, Rect)>,
    /// The lightbox close button's screen rect, when one was painted.
    last_lightbox_close: Option<Rect>,
    /// Last object focus reported to the runtime, so only changes are sent.
    last_focus_object: Option<String>,
}

impl Default for PlayerFrame {
    fn default() -> Self {
        Self {
            reduced_motion: false,
            ended: false,
            captions_on: false,
            volume: 1.0,
            last_volume: 1.0,
            volume_changed: false,
            seek_drag: None,
            seek_drag_slide: None,
            notice: None,
            can_speed: false,
            can_fullscreen: false,
            fullscreen: false,
            speed_open: false,
            fullscreen_toggle: false,
            last_logo: None,
            drawer_view: DrawerView::default(),
            focus_glossary_search: false,
            caption: None,
            last_activity: 0.0,
            last_time: 0.0,
            initialized: false,
            strip_opacity: 1.0,
            drawer_open: false,
            drawer_progress: 0.0,
            last_strip_rect: None,
            last_drawer_rect: None,
            last_ended_card: None,
            last_ended_heading: None,
            last_ended_small: None,
            last_notice_rect: None,
            last_review_rect: None,
            notice_dismissed: false,
            focus: crate::focus::FocusManager::new(),
            last_chrome: Vec::new(),
            last_lightbox_close: None,
            last_focus_object: None,
        }
    }
}

impl PlayerFrame {
    /// Create a frame with the strip visible and the drawer closed.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the strip rect from the most recent frame, if it was painted.
    #[must_use]
    pub fn strip_rect(&self) -> Option<Rect> {
        self.last_strip_rect
    }

    /// Return the drawer rect from the most recent frame, if it was painted.
    #[must_use]
    pub fn drawer_rect(&self) -> Option<Rect> {
        self.last_drawer_rect
    }

    /// Return the completion card rect from the most recent ended frame, if painted.
    #[must_use]
    pub fn ended_card_rect(&self) -> Option<Rect> {
        self.last_ended_card
    }

    /// Return the completion heading and hint text rects from the most recent ended frame.
    #[must_use]
    pub fn ended_text_rects(&self) -> Option<(Rect, Rect)> {
        Some((self.last_ended_heading?, self.last_ended_small?))
    }

    /// Where the player logo was drawn on the most recent frame.
    #[must_use]
    pub fn logo_rect(&self) -> Option<Rect> {
        self.last_logo
    }

    /// The drawer's tab, search text and open term.
    #[must_use]
    pub fn drawer_view(&self) -> &DrawerView {
        &self.drawer_view
    }

    /// Whether the outline drawer is open.
    #[must_use]
    pub fn drawer_open(&self) -> bool {
        self.drawer_open
    }

    /// Whether captions are turned on.
    #[must_use]
    pub fn captions_on(&self) -> bool {
        self.captions_on
    }

    /// Player volume for this session, `0.0`–`1.0`.
    #[must_use]
    pub fn volume(&self) -> f32 {
        self.volume
    }

    /// Set the player volume, clamped and guarded against hostile floats.
    pub fn set_volume(&mut self, volume: f32) {
        let volume = coursecraft_playback::sanitize_volume(volume);
        if volume > 0.0 {
            self.last_volume = volume;
        }
        if (volume - self.volume).abs() > f32::EPSILON {
            self.volume = volume;
            self.volume_changed = true;
        }
    }

    /// Mute when audible, otherwise restore the last audible level.
    pub fn toggle_mute(&mut self) {
        if self.volume > 0.0 {
            self.last_volume = self.volume;
            self.set_volume(0.0);
        } else {
            let restore = if self.last_volume > 0.0 {
                self.last_volume
            } else {
                1.0
            };
            self.set_volume(restore);
        }
    }

    /// The transport controls the strip laid out on the most recent frame.
    #[must_use]
    pub fn chrome_visibility(&self) -> ChromeVisibility {
        self.focus.chrome_visibility()
    }

    /// The notice text currently shown, if a notice is set, non-empty and not
    /// dismissed for the session.
    #[must_use]
    pub fn showing_notice(&self) -> Option<&str> {
        let text = self.notice.as_deref()?;
        if self.notice_dismissed || text.is_empty() {
            None
        } else {
            Some(text)
        }
    }

    /// The notice banner rect from the most recent frame, if it was painted.
    #[must_use]
    pub fn notice_rect(&self) -> Option<Rect> {
        self.last_notice_rect
    }

    /// The review banner rect from the most recent frame, if it was painted.
    #[must_use]
    pub fn review_rect(&self) -> Option<Rect> {
        self.last_review_rect
    }

    /// Set the caption to present on the next frame.
    ///
    /// The app passes the current [`crate::media_driver::CaptionFrame`]; the
    /// frame draws audio captions itself and leaves video to the web layer.
    pub fn set_caption_frame(&mut self, caption: Option<crate::media_driver::CaptionFrame>) {
        self.caption = caption;
    }

    /// The keyboard focus manager, for tests and future screen-reader work.
    #[must_use]
    pub fn focus(&self) -> &crate::focus::FocusManager {
        &self.focus
    }

    /// Mutable access to the focus manager, so the web layer can feed it events.
    pub fn focus_mut(&mut self) -> &mut crate::focus::FocusManager {
        &mut self.focus
    }

    /// The screen rect of `button` in the most recent frame, when it was laid out.
    #[must_use]
    pub fn chrome_rect(&self, button: &crate::focus::ChromeButton) -> Option<Rect> {
        self.last_chrome
            .iter()
            .find(|(painted, _)| painted == button)
            .map(|(_, rect)| *rect)
    }

    /// The lightbox close button's screen rect, when one was painted.
    #[must_use]
    pub fn lightbox_close_rect(&self) -> Option<Rect> {
        self.last_lightbox_close
    }

    /// Force the strip hidden until the next pointer or keyboard activity.
    ///
    /// Used by headless snapshots to reproduce the idle state exactly.
    pub fn hide_strip(&mut self) {
        self.initialized = true;
        self.last_activity = f64::NEG_INFINITY;
        self.strip_opacity = 0.0;
    }

    /// Reset the presentation state for a fresh run of the course.
    ///
    /// The chrome keeps session preferences (captions, reduced motion, notice),
    /// but forgets the ended card, drawer, focus ring and measured rects so a
    /// restart starts cleanly on the first slide rather than looking finished.
    pub fn restart(&mut self) {
        self.ended = false;
        self.caption = None;
        self.last_activity = 0.0;
        self.last_time = 0.0;
        self.initialized = false;
        self.strip_opacity = 1.0;
        self.drawer_open = false;
        self.drawer_progress = 0.0;
        self.last_strip_rect = None;
        self.last_drawer_rect = None;
        self.last_ended_card = None;
        self.last_ended_heading = None;
        self.last_ended_small = None;
        self.last_notice_rect = None;
        self.last_review_rect = None;
        self.drawer_view = DrawerView::default();
        self.focus = crate::focus::FocusManager::new();
        self.last_chrome.clear();
        self.last_lightbox_close = None;
        self.seek_drag = None;
        self.seek_drag_slide = None;
    }

    /// Open the outline drawer fully, without animating. Used by snapshots.
    pub fn open_drawer(&mut self) {
        self.drawer_open = true;
        self.drawer_progress = 1.0;
    }

    /// Present the player chrome inside `ui`.
    #[allow(clippy::cognitive_complexity)]
    pub fn show(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        view: &mut SlideView,
        media: Option<(&mut MediaCache, &dyn MediaSource)>,
    ) -> FrameOutput {
        let requested = ui.available_size();
        if !requested.x.is_finite()
            || !requested.y.is_finite()
            || requested.x <= 0.0
            || requested.y <= 0.0
        {
            return FrameOutput::default();
        }
        let (full, _) = ui.allocate_exact_size(requested, Sense::hover());
        if !valid_rect(full) {
            return FrameOutput::default();
        }
        ui.painter()
            .rect_filled(full, CornerRadius::ZERO, PLAYER_GROUND);

        let started = player.state().started;
        let settings = &player.effective_player();
        let show_menu = settings.show_menu && started;
        let show_nav = settings.show_nav && started;
        let now = ui.input(|i| i.time);
        let dt = if self.initialized {
            (now - self.last_time).clamp(0.0, 0.1) as f32
        } else {
            0.0
        };
        if !self.initialized {
            self.last_activity = now;
            self.initialized = true;
        }
        self.last_time = now;

        // The dismiss button leads the focus list while the banner is showing,
        // from the same state the banner below is drawn from.
        let notice_showing = self.showing_notice().is_some();
        self.focus.set_notice_visible(notice_showing);

        let mut media = media;
        let logo = if self.ended {
            None
        } else {
            logo_texture(ui, player, media.as_mut())
        };
        let layout = if self.ended {
            None
        } else {
            strip_layout(
                ui,
                player,
                full,
                &StripOptions {
                    show_menu,
                    show_nav,
                    can_speed: self.can_speed,
                    can_fullscreen: self.can_fullscreen,
                    logo_size: logo.as_ref().map(|(_, size)| *size),
                },
            )
        };
        self.last_logo = layout.as_ref().and_then(|l| l.logo);
        let slide_id = player.current_slide().map(|slide| slide.id.clone());
        if self.seek_drag_slide != slide_id {
            self.seek_drag = None;
            self.seek_drag_slide = slide_id;
        }
        self.focus.set_chrome_visibility(
            layout
                .as_ref()
                .map_or(ChromeVisibility::none(), |l| l.visible),
        );
        self.last_chrome.clear();
        if let Some(layout) = &layout {
            for (button, rect) in [
                (crate::focus::ChromeButton::PlayPause, layout.play),
                (crate::focus::ChromeButton::Replay, layout.replay),
                (crate::focus::ChromeButton::Seek, layout.seek),
                (crate::focus::ChromeButton::Mute, layout.volume),
                (crate::focus::ChromeButton::Volume, layout.volume_slider),
                (crate::focus::ChromeButton::Captions, layout.captions),
                (crate::focus::ChromeButton::Speed, layout.speed),
                (crate::focus::ChromeButton::Outline, layout.outline),
                (crate::focus::ChromeButton::Previous, layout.previous),
                (crate::focus::ChromeButton::Next, layout.next),
                (crate::focus::ChromeButton::Fullscreen, layout.fullscreen),
            ] {
                if let Some(rect) = rect {
                    self.last_chrome.push((button, rect));
                }
            }
        }
        let pointer_hovered = view.hovered_object().map(str::to_owned);
        let previous_hover = self.focus.hovered_object().map(str::to_owned);
        self.focus.set_drawer_view(self.drawer_view.clone());
        let mut inputs = self.handle_keyboard(ui, player, show_menu, show_nav);
        // Once the course has ended the card covers the slide and the strip is
        // hidden, so there is nothing left to focus.
        if !self.ended {
            inputs.extend(self.focus.update_with_captions(
                ui,
                player,
                crate::focus::ChromeOptions {
                    show_nav,
                    show_menu,
                },
                &mut self.drawer_open,
                pointer_hovered.as_deref(),
                &mut self.captions_on,
                &mut self.notice_dismissed,
            ));
            self.drawer_view = self.focus.drawer_view().clone();
            if let Some(request) = self.focus.take_chrome_request() {
                match request {
                    crate::focus::ChromeRequest::ToggleMute => self.toggle_mute(),
                    crate::focus::ChromeRequest::NudgeVolume(delta) => {
                        self.set_volume(self.volume + delta);
                    }
                    crate::focus::ChromeRequest::SetVolume(volume) => self.set_volume(volume),
                    crate::focus::ChromeRequest::ToggleFullscreen => {
                        self.fullscreen_toggle = true;
                    }
                    crate::focus::ChromeRequest::FocusGlossarySearch => {
                        self.focus_glossary_search = true;
                    }
                }
            }
        }
        let focused_object = match self.focus.focused() {
            Some(crate::focus::FocusTarget::Object(id)) => Some(id.clone()),
            _ => None,
        };
        // Object focus only matters while the learner is working by keyboard;
        // report it when it changes, including clearing it.
        if self.focus.keyboard_mode() && focused_object != self.last_focus_object {
            self.last_focus_object = focused_object.clone();
            inputs.insert(
                0,
                Input::FocusChanged {
                    object: focused_object,
                },
            );
        }
        if previous_hover.as_deref() != self.focus.hovered_object()
            && let Some(old) = previous_hover
        {
            view.release_keyboard_hover(&old);
        }
        view.set_keyboard_hover(self.focus.hovered_object());
        view.set_keyboard_drag(self.focus.drag_visual());

        let pointer = ui.input(|i| i.pointer.interact_pos());
        let over_strip =
            pointer.is_some_and(|pos| layout.as_ref().is_some_and(|l| l.rect.contains(pos)));
        let focused = strip_has_focus(ui) || self.focus.chrome_focused().is_some();
        if activity(ui) || self.focus.chrome_focused().is_some() {
            self.last_activity = now;
        }
        self.update_visibility(dt, now, over_strip, focused, player.player_paused());

        let drawer = drawer_rect(full, self.drawer_progress);
        let mut covered = Vec::new();
        if self.strip_opacity > 0.01
            && let Some(l) = &layout
        {
            covered.push(l.rect);
        }
        if self.drawer_progress > 0.01 && show_menu {
            // The scrim covers the whole frame, so nothing reaches the slide.
            covered.push(full);
        }
        let (notice_text, notice_banner) = self.notice_area(ui, player, full, &mut covered);
        let review_text = review_banner_text(player);
        let review_base = match &notice_banner {
            Some(l) => Rect::from_min_max(Pos2::new(full.min.x, l.rect.max.y), full.max),
            None => full,
        };
        let review_banner = review_text
            .as_deref()
            .and_then(|text| notice_layout(ui, review_base, player, text));
        if let Some(l) = &review_banner {
            covered.push(l.rect);
        }
        let slide_rect = slide_rect(full, player);
        let close_rect = if player.lightbox_open() {
            crate::view::lightbox_box_rect(slide_rect, player.course().slide_size)
                .and_then(crate::view::lightbox_close_rect)
        } else {
            None
        };
        if let Some(rect) = close_rect {
            // The × is drawn above the canvas; keep its press off the box.
            covered.push(rect);
        }
        view.set_reduced_motion(self.reduced_motion);
        inputs.extend(show_slide(ui, player, view, media, slide_rect, &covered));
        let transform = view.last_transform();
        let focus_transform = if player.lightbox_open() {
            view.lightbox_transform()
        } else {
            transform
        };
        if player.lightbox_open() && !self.focus.lightbox_open() {
            let opener = view
                .last_clicked_object()
                .map(|id| crate::focus::FocusTarget::Object(id.to_owned()));
            self.focus.remember_lightbox_opener(opener);
        }
        // Close button above the box, then the focus ring on top of it.
        self.last_lightbox_close = close_rect;
        if let Some(rect) = close_rect {
            let clicked = draw_lightbox_close(ui, player, rect, &self.focus);
            if clicked {
                inputs.push(Input::CloseLightbox);
            }
            self.last_chrome
                .push((crate::focus::ChromeButton::CloseLightbox, rect));
        } else {
            self.last_lightbox_close = None;
        }
        if !self.ended {
            self.focus
                .paint(ui, player, view.last_items(), focus_transform.as_ref());
        }
        self.show_captions(ui, layout.as_ref(), slide_rect, transform.as_ref());

        if self.ended {
            let accent = parse_color(&player.course().theme.accent)
                .unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB));
            if let Some(layout) = draw_ended(ui, slide_rect, accent, player.course()) {
                self.last_ended_card = Some(layout.card);
                self.last_ended_heading = Some(layout.heading);
                self.last_ended_small = Some(layout.small);
            } else {
                self.last_ended_card = None;
                self.last_ended_heading = None;
                self.last_ended_small = None;
            }
        }
        if self.drawer_progress > 0.01 && show_menu {
            draw_scrim(ui, full, drawer, self.drawer_progress);
        }
        if let Some(l) = &layout {
            self.last_strip_rect = Some(l.rect);
            if self.strip_opacity > 0.01 {
                draw_strip(
                    ui,
                    player,
                    l,
                    self.strip_opacity,
                    &mut self.drawer_open,
                    show_nav,
                    &mut inputs,
                    &self.focus,
                    &mut self.captions_on,
                    &mut self.volume,
                    &mut self.last_volume,
                    &mut self.volume_changed,
                    &mut self.seek_drag,
                    &mut self.speed_open,
                    &mut self.fullscreen_toggle,
                    self.fullscreen,
                    logo.as_ref().map(|(texture, _)| texture),
                );
            } else {
                self.last_strip_rect = None;
            }
        } else {
            self.last_strip_rect = None;
        }

        if self.drawer_progress > 0.01 && show_menu {
            self.last_drawer_rect = Some(drawer);
            draw_drawer(
                ui,
                player,
                drawer,
                self.drawer_progress,
                show_nav,
                &mut self.drawer_open,
                &mut inputs,
                &self.focus,
                &mut self.last_chrome,
                &mut self.drawer_view,
                &mut self.focus_glossary_search,
            );
        } else {
            self.last_drawer_rect = None;
        }

        if self.drawer_open
            && let Some(pos) = pointer
            && ui.input(|i| i.pointer.any_pressed())
            && !drawer.contains(pos)
            && !layout.as_ref().is_some_and(|l| l.rect.contains(pos))
            && !notice_banner.as_ref().is_some_and(|l| l.rect.contains(pos))
            && !review_banner.as_ref().is_some_and(|l| l.rect.contains(pos))
        {
            self.drawer_open = false;
        }

        self.show_review(ui, player, review_text, review_banner, &mut inputs);
        self.show_notice(ui, player, notice_text, notice_banner);

        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(16));
        let volume = if self.volume_changed {
            self.volume_changed = false;
            Some(self.volume)
        } else {
            None
        };
        let fullscreen_toggle = std::mem::take(&mut self.fullscreen_toggle);
        FrameOutput {
            inputs,
            slide_rect,
            volume,
            fullscreen_toggle,
        }
    }

    /// The notice banner's text and layout for this frame, adding the banner's
    /// rect to `covered` so presses on it never reach the slide.
    fn notice_area(
        &self,
        ui: &Ui,
        player: &Player<'_>,
        full: Rect,
        covered: &mut Vec<Rect>,
    ) -> (Option<String>, Option<NoticeLayout>) {
        let notice_text = self.showing_notice().map(str::to_owned);
        let notice_layout = notice_text
            .as_deref()
            .and_then(|text| notice_layout(ui, full, player, text));
        if let Some(l) = &notice_layout {
            covered.push(l.rect);
        }
        (notice_text, notice_layout)
    }

    /// Paint the notice banner when it is showing, recording its screen rects
    /// for the DOM layer and hiding it for the session when dismissed.
    fn show_notice(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        notice_text: Option<String>,
        notice_layout: Option<NoticeLayout>,
    ) {
        match (notice_text.as_deref(), notice_layout.as_ref()) {
            (Some(text), Some(l)) => {
                let warning = palette(ui.ctx()).warning;
                if draw_notice(
                    ui,
                    player,
                    l,
                    text,
                    warning,
                    Icon::Warning,
                    "dismiss-notice",
                    &crate::focus::ChromeButton::DismissNotice,
                    player.course().chrome_string("dismissNotice"),
                    &self.focus,
                ) {
                    self.notice_dismissed = true;
                }
                self.last_notice_rect = Some(l.rect);
                self.last_chrome
                    .push((crate::focus::ChromeButton::DismissNotice, l.close));
            }
            _ => self.last_notice_rect = None,
        }
    }

    /// Paint the review banner when reviewing, recording its rect for the DOM
    /// layer. It is never dismissible; its button sends `Input::ExitReview`.
    fn show_review(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        review_text: Option<String>,
        review_layout: Option<NoticeLayout>,
        inputs: &mut Vec<Input>,
    ) {
        match (review_text.as_deref(), review_layout.as_ref()) {
            (Some(text), Some(l)) => {
                let accent = parse_color(&player.course().theme.accent)
                    .unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB));
                if draw_notice(
                    ui,
                    player,
                    l,
                    text,
                    accent,
                    Icon::CheckCircle,
                    "exit-review",
                    &crate::focus::ChromeButton::ExitReview,
                    player.course().chrome_string("exitReview"),
                    &self.focus,
                ) {
                    inputs.push(Input::ExitReview);
                }
                self.last_review_rect = Some(l.rect);
                self.last_chrome
                    .push((crate::focus::ChromeButton::ExitReview, l.close));
            }
            _ => self.last_review_rect = None,
        }
    }

    fn handle_keyboard(
        &mut self,
        ui: &mut Ui,
        player: &Player<'_>,
        show_menu: bool,
        show_nav: bool,
    ) -> Vec<Input> {
        self.focus.flush_external_keys(ui);
        if ui.ctx().text_edit_focused() {
            return Vec::new();
        }
        // Menu and caption keys are consumed here so a trigger's key forwarding
        // cannot also see them. Escape is left unconsumed when it closes
        // nothing, so the host (for example the editor preview) can use it.
        let (left, right, menu, escape, captions) = ui.input_mut(|i| {
            (
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.consume_key(egui::Modifiers::NONE, egui::Key::M),
                i.key_pressed(egui::Key::Escape),
                i.consume_key(egui::Modifiers::NONE, egui::Key::C),
            )
        });
        if player.lightbox_open() {
            if escape {
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
                });
                return vec![Input::CloseLightbox];
            }
            return Vec::new();
        }
        if captions && !self.ended {
            self.captions_on = !self.captions_on;
        }
        let left = left && !self.focus.captures_arrow(egui::Key::ArrowLeft);
        let right = right && !self.focus.captures_arrow(egui::Key::ArrowRight);
        if menu && show_menu {
            self.drawer_open = !self.drawer_open;
        }
        if escape && self.drawer_open {
            self.drawer_open = false;
            ui.input_mut(|i| {
                i.consume_key(egui::Modifiers::NONE, egui::Key::Escape);
            });
        }
        if !player.state().started || self.drawer_open {
            return Vec::new();
        }
        let Some(slide) = player.current_slide() else {
            return Vec::new();
        };
        let (is_first, is_last) = nav_limits(
            player.play_order().iter().map(|s| s.id.as_str()),
            slide.id.as_str(),
        );
        let mut inputs = Vec::new();
        if show_nav {
            let mut used_left = false;
            let mut used_right = false;
            if left && !is_first {
                inputs.push(Input::Previous);
                used_left = true;
            }
            if right && !is_last && player.can_next() {
                inputs.push(Input::Next);
                used_right = true;
            }
            // Navigation owns the arrow it used; other arrows stay for key triggers
            // and for the focused control.
            if used_left {
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft);
                });
            }
            if used_right {
                ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight);
                });
            }
        }
        // Arrow keys that neither navigation nor a focused control used can
        // drive `key_pressed` triggers.
        for (key, name) in [
            (egui::Key::ArrowUp, "arrow_up"),
            (egui::Key::ArrowDown, "arrow_down"),
            (egui::Key::ArrowLeft, "arrow_left"),
            (egui::Key::ArrowRight, "arrow_right"),
        ] {
            if self.focus.captures_arrow(key) || !player.has_key_trigger(name) {
                continue;
            }
            let count = ui.input_mut(|i| i.count_and_consume_key(egui::Modifiers::NONE, key));
            for _ in 0..count {
                inputs.push(Input::KeyPressed {
                    key: name.to_owned(),
                });
            }
        }
        inputs
    }

    fn show_captions(
        &self,
        ui: &Ui,
        layout: Option<&StripLayout>,
        slide_rect: Rect,
        transform: Option<&crate::layout::SlideTransform>,
    ) {
        if self.ended || !self.captions_on {
            return;
        }
        let Some(caption) = &self.caption else {
            return;
        };
        if caption.kind != coursecraft_playback::MediaKind::Audio {
            return;
        }
        let strip_top = if self.strip_opacity > 0.01 {
            layout.map(|layout| layout.rect.top())
        } else {
            None
        };
        draw_caption(ui, &caption.text, slide_rect, transform, strip_top);
    }

    fn update_visibility(
        &mut self,
        dt: f32,
        now: f64,
        over_strip: bool,
        focused: bool,
        paused: bool,
    ) {
        let idle = now - self.last_activity;
        let want_visible = self.reduced_motion
            || self.drawer_open
            || self.speed_open
            || over_strip
            || focused
            || paused
            || idle < AUTO_HIDE_SECONDS;
        let target = if want_visible { 1.0 } else { 0.0 };
        if self.reduced_motion {
            self.strip_opacity = target;
        } else if dt > 0.0 {
            let step = dt / FADE_SECONDS;
            if self.strip_opacity < target {
                self.strip_opacity = (self.strip_opacity + step).min(1.0);
            } else if self.strip_opacity > target {
                self.strip_opacity = (self.strip_opacity - step).max(0.0);
            }
        }
        let drawer_target = if self.drawer_open { 1.0 } else { 0.0 };
        if self.reduced_motion {
            self.drawer_progress = drawer_target;
        } else if dt > 0.0 {
            let step = dt / DRAWER_SECONDS;
            if self.drawer_progress < drawer_target {
                self.drawer_progress = (self.drawer_progress + step).min(1.0);
            } else if self.drawer_progress > drawer_target {
                self.drawer_progress = (self.drawer_progress - step).max(0.0);
            }
        }
    }
}

fn strip_has_focus(ui: &Ui) -> bool {
    ui.memory(|m| {
        m.focused().is_some_and(|id| {
            [
                "play-pause",
                "replay",
                "seek",
                "mute",
                "volume",
                "captions",
                "outline",
                "previous",
                "next",
                "speed",
                "fullscreen",
            ]
            .iter()
            .any(|name| id == strip_id(name))
        })
    })
}

fn strip_id(name: &str) -> Id {
    Id::unique(("coursecraft-player-strip", name))
}

fn activity(ui: &Ui) -> bool {
    ui.input(|i| {
        i.pointer.is_moving()
            || i.pointer.any_pressed()
            || i.events.iter().any(|event| {
                matches!(
                    event,
                    Event::Key { pressed: true, .. }
                        | Event::PointerButton { .. }
                        | Event::PointerMoved(_)
                        | Event::PointerGone
                        | Event::Touch { .. }
                        | Event::MouseMoved(_)
                )
            })
    })
}

fn valid_rect(rect: Rect) -> bool {
    rect.is_positive() && rect.is_finite()
}

/// Draw one caption line box over the slide, above the strip.
fn draw_caption(
    ui: &Ui,
    text: &str,
    slide_rect: Rect,
    transform: Option<&crate::layout::SlideTransform>,
    strip_top: Option<f32>,
) {
    if !valid_rect(slide_rect) || text.is_empty() {
        return;
    }
    let scale = transform.map_or(1.0, crate::layout::SlideTransform::scale);
    if !scale.is_finite() || scale <= 0.0 {
        return;
    }
    let font = FontId::new(
        (SIZE_BODY * scale).clamp(1.0, 400.0),
        FontFamily::Name("inter".into()),
    );
    let max_width = (slide_rect.width() * 0.8).max(1.0);
    let painter = ui.painter().with_clip_rect(slide_rect);
    let galley = painter.layout(text.to_owned(), font, Color32::WHITE, max_width);
    let padding = 6.0 * scale;
    let mut bottom = slide_rect.bottom() - 24.0 * scale;
    if let Some(top) = strip_top
        && top.is_finite()
    {
        bottom = bottom.min(top - 12.0);
    }
    let size = galley.size();
    let rect = Rect::from_min_size(
        Pos2::new(
            slide_rect.center().x - (size.x + padding * 2.0) * 0.5,
            bottom - size.y - padding * 2.0,
        ),
        vec2(size.x + padding * 2.0, size.y + padding * 2.0),
    );
    if !valid_rect(rect) {
        return;
    }
    let radius = CornerRadius::same((4.0 * scale).clamp(0.0, 255.0) as u8);
    painter.rect_filled(rect, radius, Color32::from_black_alpha(0xC0));
    painter.galley(
        Pos2::new(rect.center().x - size.x * 0.5, rect.top() + padding),
        galley,
        Color32::WHITE,
    );
}

/// The review banner's text: the current question's place among the answered
/// questions, its result and its points. `None` when not reviewing.
pub(crate) fn review_banner_text(player: &Player<'_>) -> Option<String> {
    if !player.is_reviewing() {
        return None;
    }
    let slide = player.current_slide()?;
    let question = slide.question.as_ref()?;
    let answer = player.state().answers.get(&slide.id)?;
    let answered: Vec<&coursecraft_course::Slide> = player
        .play_order()
        .iter()
        .copied()
        .filter(|slide| slide.question.is_some() && player.state().answers.contains_key(&slide.id))
        .collect();
    let position = answered
        .iter()
        .position(|candidate| candidate.id == slide.id)?
        .saturating_add(1);
    let total = answered.len();
    let course = player.course();
    let place = fill(
        course.chrome_string("reviewQuestion"),
        &[("n", &position.to_string()), ("total", &total.to_string())],
    );
    let result = if answer.correct {
        course.chrome_string("reviewCorrect")
    } else {
        course.chrome_string("reviewIncorrect")
    };
    let points = fill(
        course.chrome_string("reviewPoints"),
        &[
            ("points", &answer.points.to_string()),
            ("max", &question.max_points().to_string()),
        ],
    );
    Some(format!(
        "{} · {} · {} · {}",
        course.chrome_string("reviewing"),
        place,
        result,
        points
    ))
}

/// Screen layout of the notice banner: the whole banner and its close button.
struct NoticeLayout {
    rect: Rect,
    close: Rect,
}

/// Measure the notice banner across the top of the player. The text wraps at
/// the width left over from the icon and the close button, so the banner grows
/// taller on narrow frames.
fn notice_layout(ui: &Ui, full: Rect, player: &Player<'_>, text: &str) -> Option<NoticeLayout> {
    if !valid_rect(full) || text.is_empty() {
        return None;
    }
    let (_, text_color, _) = bar_colors(player);
    let font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let text_left = full.left() + NOTICE_PAD + ICON_TOOLBAR + NOTICE_GAP;
    let close_left = full.right() - NOTICE_PAD - NOTICE_BUTTON;
    let wrap_width = (close_left - NOTICE_GAP - text_left).max(1.0);
    let galley = ui
        .painter()
        .layout(text.to_owned(), font, text_color, wrap_width);
    let text_height = galley.size().y;
    if !text_height.is_finite() {
        return None;
    }
    let content = text_height.max(NOTICE_BUTTON).max(ICON_TOOLBAR).max(1.0);
    let height = (NOTICE_ACCENT + NOTICE_PAD + content + NOTICE_PAD).min(full.height().max(0.0));
    let rect = Rect::from_min_size(full.min, vec2(full.width(), height));
    let close_top = (full.top() + NOTICE_ACCENT + NOTICE_PAD + (content - NOTICE_BUTTON) * 0.5)
        .clamp(full.top(), (rect.bottom() - NOTICE_BUTTON).max(full.top()));
    let close = Rect::from_min_size(
        Pos2::new(close_left, close_top),
        vec2(NOTICE_BUTTON, NOTICE_BUTTON),
    );
    Some(NoticeLayout { rect, close })
}

/// Paint a banner: the strip's bar with a coloured rule, an icon, the wrapped
/// text, and a button. Returns whether the button was activated. Only the
/// button takes a pointer.
#[allow(clippy::too_many_arguments)]
fn draw_notice(
    ui: &mut Ui,
    player: &Player<'_>,
    layout: &NoticeLayout,
    text: &str,
    rule_color: Color32,
    icon: Icon,
    button_id: &'static str,
    action: &crate::focus::ChromeButton,
    tooltip: &str,
    focus: &crate::focus::FocusManager,
) -> bool {
    let (bar, text_color, accent) = bar_colors(player);
    let mut child = ui.new_child(UiBuilder::new().max_rect(layout.rect).id_salt(button_id));
    child.set_clip_rect(layout.rect);
    let painter = child.painter();
    painter.rect_filled(layout.rect, CornerRadius::ZERO, bar);
    let rule = Rect::from_min_max(
        layout.rect.min,
        Pos2::new(layout.rect.right(), layout.rect.top() + NOTICE_ACCENT),
    );
    painter.rect_filled(rule, CornerRadius::ZERO, rule_color);

    let content = Rect::from_min_max(
        Pos2::new(layout.rect.left(), layout.rect.top() + NOTICE_ACCENT),
        layout.rect.max,
    );
    let text_left = layout.rect.left() + NOTICE_PAD + ICON_TOOLBAR + NOTICE_GAP;
    let wrap_width = (layout.close.left() - NOTICE_GAP - text_left).max(1.0);
    let font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let galley = painter.layout(text.to_owned(), font, text_color, wrap_width);
    painter.galley(
        Pos2::new(text_left, content.center().y - galley.size().y * 0.5),
        galley,
        text_color,
    );
    painter.text(
        Pos2::new(
            layout.rect.left() + NOTICE_PAD + ICON_TOOLBAR * 0.5,
            content.center().y,
        ),
        Align2::CENTER_CENTER,
        icon.codepoint().to_string(),
        FontId::new(ICON_TOOLBAR, FontFamily::Name("phosphor".into())),
        rule_color,
    );
    let activated = button(
        &child,
        layout.close,
        button_id,
        Icon::X,
        text_color,
        true,
        None,
        tooltip,
    );
    focus.paint_chrome(&child, layout.close, action, accent, bar);
    activated
}

fn slide_rect(full: Rect, player: &Player<'_>) -> Rect {
    let size = player.course().slide_size;
    let slide_w = size.w as f32;
    let slide_h = size.h as f32;
    if slide_w <= 0.0 || slide_h <= 0.0 || !slide_w.is_finite() || !slide_h.is_finite() {
        return full;
    }
    let slide_aspect = slide_w / slide_h;
    let full_aspect = full.width() / full.height();
    if !slide_aspect.is_finite() || !full_aspect.is_finite() {
        return full;
    }
    if full_aspect > slide_aspect {
        let height = full.height();
        let width = height * slide_aspect;
        Rect::from_center_size(full.center(), vec2(width, height))
    } else {
        let width = full.width();
        let height = width / slide_aspect;
        Rect::from_center_size(full.center(), vec2(width, height))
    }
}

fn show_slide(
    ui: &mut Ui,
    player: &Player<'_>,
    view: &mut SlideView,
    media: Option<(&mut MediaCache, &dyn MediaSource)>,
    rect: Rect,
    covered: &[Rect],
) -> Vec<Input> {
    if !valid_rect(rect) {
        return Vec::new();
    }
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(rect)
            .id_salt("coursecraft-player-slide"),
    );
    child.set_clip_rect(rect);
    view.show_covered(&mut child, player, media, covered)
}

struct StripLayout {
    rect: Rect,
    logo: Option<Rect>,
    play: Option<Rect>,
    replay: Option<Rect>,
    seek: Option<Rect>,
    time: Option<Rect>,
    volume: Option<Rect>,
    volume_slider: Option<Rect>,
    captions: Option<Rect>,
    outline: Option<Rect>,
    title: Option<Rect>,
    counter: Option<Rect>,
    previous: Option<Rect>,
    next: Option<Rect>,
    speed: Option<Rect>,
    fullscreen: Option<Rect>,
    title_text: String,
    counter_text: String,
    time_text: String,
    can_seek: bool,
    at_end: bool,
    visible: ChromeVisibility,
}

fn measure(ui: &Ui, text: &str, font: &FontId) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_owned(), font.clone(), Color32::WHITE)
        .size()
        .x
}

#[allow(clippy::too_many_arguments)]
fn strip_width(
    started: bool,
    show_seekbox: bool,
    show_captions: bool,
    show_menu: bool,
    show_nav: bool,
    show_title: bool,
    show_counter: bool,
    show_volume: bool,
    show_volume_slider: bool,
    show_time: bool,
    show_replay: bool,
    show_speed: bool,
    show_fullscreen: bool,
    logo_width: f32,
    seek_width: f32,
    title_width: f32,
    counter_width: f32,
    time_width: f32,
) -> f32 {
    let mut widths: Vec<f32> = Vec::new();
    if logo_width > 0.0 {
        widths.push(logo_width);
    }
    if started {
        widths.push(STRIP_BUTTON);
    }
    if started && show_replay {
        widths.push(STRIP_BUTTON);
    }
    if show_seekbox {
        widths.push(seek_width);
    }
    if show_time {
        widths.push(time_width);
    }
    if show_volume {
        widths.push(STRIP_BUTTON);
    }
    if started && show_volume_slider {
        widths.push(VOLUME_SLIDER_WIDTH);
    }
    if show_captions {
        widths.push(STRIP_BUTTON);
    }
    if show_menu {
        widths.push(STRIP_BUTTON);
    }
    if show_title {
        widths.push(title_width);
    }
    if show_counter {
        widths.push(counter_width);
    }
    if show_nav {
        widths.push(STRIP_BUTTON);
        widths.push(STRIP_BUTTON);
    }
    if show_speed {
        widths.push(SPEED_BUTTON);
    }
    if show_fullscreen {
        widths.push(STRIP_BUTTON);
    }
    let gaps = widths.len().saturating_sub(1) as f32 * STRIP_GAP;
    2.0 * STRIP_PAD + widths.iter().sum::<f32>() + gaps
}

/// What the host and the frame decided before the strip is laid out.
struct StripOptions {
    show_menu: bool,
    show_nav: bool,
    can_speed: bool,
    can_fullscreen: bool,
    /// The drawn size of the player logo, once its picture has loaded.
    logo_size: Option<Vec2>,
}

fn strip_layout(
    ui: &Ui,
    player: &Player<'_>,
    full: Rect,
    options: &StripOptions,
) -> Option<StripLayout> {
    let StripOptions {
        show_menu,
        show_nav,
        can_speed,
        can_fullscreen,
        logo_size,
    } = *options;
    let settings = player.effective_player();
    let logo_width = logo_size.map_or(0.0, |size| size.x);
    let max_width = full.width() - 2.0 * STRIP_MARGIN;
    if max_width < STRIP_HEIGHT || full.height() < STRIP_HEIGHT {
        return None;
    }
    let slide = player.current_slide()?;
    let total = player.play_order().len();
    let index = player.slide_position(&slide.id)?;
    let started = player.state().started;
    let show_seekbox = settings.show_seekbar && started;
    let info = player.seek_info();
    let title_text = player.display_text(&slide.title);
    let counter_text = format!("{} / {}", index.saturating_add(1), total);
    let shown_ms = if info.duration_ms > 0 {
        info.elapsed_ms.min(info.duration_ms)
    } else {
        info.elapsed_ms
    };
    let time_text = format!("{} / {}", format_ms(shown_ms), format_ms(info.duration_ms));

    let title_font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let counter_font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let time_font = FontId::new(SIZE_MONO, FontFamily::Monospace);
    let title_width = measure(ui, &title_text, &title_font).min(TITLE_MAX);
    let counter_width = measure(ui, &counter_text, &counter_font);
    let widest_time = format!(
        "{} / {}",
        format_ms(info.duration_ms),
        format_ms(info.duration_ms)
    );
    let time_width = measure(ui, &widest_time, &time_font);
    let show_captions = settings.show_captions && crate::media_driver::slide_has_captions(slide);
    let show_volume = settings.show_volume && started;

    let mut show_title = settings.show_title;
    let mut show_counter = true;
    let mut show_volume_slider = show_volume;
    let mut show_time = show_seekbox;
    let mut show_replay = started;
    let mut show_speed = can_speed && started && settings.show_speed;
    let mut show_fullscreen = can_fullscreen && started && settings.show_fullscreen;

    loop {
        let seek_width = if show_seekbox { SEEK_MIN } else { 0.0 };
        let needed = strip_width(
            started,
            show_seekbox,
            show_captions,
            show_menu,
            show_nav,
            show_title,
            show_counter,
            show_volume,
            show_volume_slider,
            show_time,
            show_replay,
            show_speed,
            show_fullscreen,
            logo_width,
            seek_width,
            title_width,
            counter_width,
            time_width,
        );
        if needed <= max_width {
            break;
        }
        if show_title {
            show_title = false;
        } else if show_counter {
            show_counter = false;
        } else if show_speed {
            show_speed = false;
        } else if show_fullscreen {
            show_fullscreen = false;
        } else if show_volume_slider {
            show_volume_slider = false;
        } else if show_time {
            show_time = false;
        } else if show_replay {
            show_replay = false;
        } else {
            break;
        }
    }

    let fixed_without_seek = strip_width(
        started,
        true,
        show_captions,
        show_menu,
        show_nav,
        show_title,
        show_counter,
        show_volume,
        show_volume_slider,
        show_time,
        show_replay,
        show_speed,
        show_fullscreen,
        logo_width,
        0.0,
        title_width,
        counter_width,
        time_width,
    );
    let seek_width = if show_seekbox {
        (max_width - fixed_without_seek).max(SEEK_MIN)
    } else {
        0.0
    };
    let content_width = strip_width(
        started,
        show_seekbox,
        show_captions,
        show_menu,
        show_nav,
        show_title,
        show_counter,
        show_volume,
        show_volume_slider,
        show_time,
        show_replay,
        show_speed,
        show_fullscreen,
        logo_width,
        seek_width,
        title_width,
        counter_width,
        time_width,
    );
    let width = if show_seekbox {
        max_width
    } else {
        content_width.min(max_width).max(STRIP_HEIGHT)
    };
    let y = (full.bottom() - STRIP_MARGIN - STRIP_HEIGHT).max(full.top());
    let rect = Rect::from_min_size(
        Pos2::new(full.center().x - width * 0.5, y),
        vec2(width, STRIP_HEIGHT),
    );

    let mut x = rect.left() + STRIP_PAD;
    let mut place = |item_width: f32| {
        let at = Rect::from_min_size(Pos2::new(x, rect.top()), vec2(item_width, STRIP_HEIGHT));
        x += item_width + STRIP_GAP;
        at
    };
    let logo = logo_size.map(|size| {
        let at = place(size.x);
        Rect::from_center_size(at.center(), size)
    });
    let play = started.then(|| place(STRIP_BUTTON));
    let replay = (started && show_replay).then(|| place(STRIP_BUTTON));
    let seek = show_seekbox.then(|| place(seek_width));
    let time = (show_seekbox && show_time).then(|| place(time_width));
    let volume = show_volume.then(|| place(STRIP_BUTTON));
    let volume_slider = (started && show_volume_slider).then(|| place(VOLUME_SLIDER_WIDTH));
    let captions = show_captions.then(|| place(STRIP_BUTTON));
    let speed = show_speed.then(|| place(SPEED_BUTTON));
    let outline = show_menu.then(|| place(STRIP_BUTTON));
    let title = show_title.then(|| place(title_width));
    let counter = show_counter.then(|| place(counter_width));
    let previous = show_nav.then(|| place(STRIP_BUTTON));
    let next = show_nav.then(|| place(STRIP_BUTTON));
    let fullscreen = show_fullscreen.then(|| place(STRIP_BUTTON));

    let visible = ChromeVisibility {
        play: play.is_some(),
        replay: replay.is_some(),
        seek: seek.is_some(),
        volume: volume.is_some(),
        volume_slider: volume_slider.is_some(),
        captions: captions.is_some(),
        outline: outline.is_some(),
        previous: previous.is_some(),
        next: next.is_some(),
        speed: speed.is_some(),
        fullscreen: fullscreen.is_some(),
    };

    Some(StripLayout {
        rect,
        logo,
        play,
        replay,
        seek,
        time,
        volume,
        volume_slider,
        captions,
        outline,
        title,
        counter,
        previous,
        next,
        speed,
        fullscreen,
        title_text,
        counter_text,
        time_text,
        can_seek: show_seekbox && info.seek_allowed && info.duration_ms > 0,
        at_end: at_slide_end(player),
        visible,
    })
}

/// The tallest the logo is drawn, and the widest, in points.
const LOGO_MAX_HEIGHT: f32 = 28.0;
const LOGO_MAX_WIDTH: f32 = 120.0;

/// The logo's texture and its drawn size (at most 28 pt high, aspect kept),
/// once the picture has loaded. `None` while loading, when it failed, or when
/// the course has no logo.
fn logo_texture(
    ui: &Ui,
    player: &Player<'_>,
    media: Option<&mut (&mut MediaCache, &dyn MediaSource)>,
) -> Option<(TextureHandle, Vec2)> {
    let src = player
        .course()
        .player
        .logo
        .as_deref()
        .filter(|s| !s.is_empty())?;
    let (cache, source) = media?;
    let target = (LOGO_MAX_WIDTH as u32 * 2, LOGO_MAX_HEIGHT as u32 * 2);
    match cache.texture_for(ui.ctx(), src, *source, target) {
        crate::media::MediaView::Ready { texture, size } => {
            let (w, h) = (size[0] as f32, size[1] as f32);
            if !(w.is_finite() && h.is_finite()) || w <= 0.0 || h <= 0.0 {
                return None;
            }
            let height = h.min(LOGO_MAX_HEIGHT);
            let width = (w * height / h).min(LOGO_MAX_WIDTH);
            let height = (h * width / w).min(LOGO_MAX_HEIGHT);
            Some((texture.clone(), vec2(width, height)))
        }
        _ => None,
    }
}

fn bar_colors(player: &Player<'_>) -> (Color32, Color32, Color32) {
    let theme = &player.course().theme;
    let bar = parse_color(&theme.player.bar).unwrap_or(DEFAULT_BAR_COLOR);
    let alpha = if bar.a() < 255 { bar.a() } else { 0xE6 };
    let bar = Color32::from_rgba_unmultiplied(bar.r(), bar.g(), bar.b(), alpha);
    let text = parse_color(&theme.player.bar_text).unwrap_or(DEFAULT_BAR_TEXT_COLOR);
    let accent = parse_color(&theme.accent).unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB));
    (bar, text, accent)
}

#[allow(clippy::too_many_arguments)]
fn draw_strip(
    ui: &mut Ui,
    player: &Player<'_>,
    layout: &StripLayout,
    opacity: f32,
    drawer_open: &mut bool,
    show_nav: bool,
    inputs: &mut Vec<Input>,
    focus: &crate::focus::FocusManager,
    captions_on: &mut bool,
    volume: &mut f32,
    last_volume: &mut f32,
    volume_changed: &mut bool,
    seek_drag: &mut Option<u32>,
    speed_open: &mut bool,
    fullscreen_toggle: &mut bool,
    fullscreen_on: bool,
    logo: Option<&TextureHandle>,
) {
    let (bar, text, accent) = bar_colors(player);
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(layout.rect)
            .id_salt("coursecraft-player-strip"),
    );
    child.multiply_opacity(opacity);
    // The shadow must not be clipped to the strip rect, or it paints a hard-edged
    // rectangle that spills past the rounded corners.
    let painter = child.painter().clone();
    let shadow = egui::epaint::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(0x66),
    };
    painter.add(shadow.as_shape(layout.rect, CornerRadius::same(STRIP_RADIUS)));
    painter.rect_filled(layout.rect, CornerRadius::same(STRIP_RADIUS), bar);

    if opacity > 0.05 {
        if let (Some(rect), Some(texture)) = (layout.logo, logo) {
            // Decorative: no label, no focus, no access node.
            child.painter().image(
                texture.id(),
                rect,
                Rect::from_min_max(Pos2::new(0.0, 0.0), Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let paused = player.player_paused();
        let secondary = Color32::from_rgba_unmultiplied(text.r(), text.g(), text.b(), 0x99);
        let dim = Color32::from_rgba_unmultiplied(text.r(), text.g(), text.b(), 0x55);

        if let Some(rect) = layout.play {
            let (icon, tooltip) = if layout.at_end {
                (
                    Icon::ArrowCounterClockwise,
                    player.course().chrome_string("replaySlide"),
                )
            } else if paused {
                (Icon::Play, player.course().chrome_string("playTooltip"))
            } else {
                (Icon::Pause, player.course().chrome_string("pauseTooltip"))
            };
            if button(&child, rect, "play-pause", icon, text, true, None, tooltip) {
                inputs.extend(play_pause_inputs(player));
            }
            focus.paint_chrome(
                &child,
                rect,
                &crate::focus::ChromeButton::PlayPause,
                accent,
                bar,
            );
        }

        if let Some(rect) = layout.replay
            && button(
                &child,
                rect,
                "replay",
                Icon::ArrowCounterClockwise,
                text,
                true,
                None,
                player.course().chrome_string("replaySlide"),
            )
        {
            inputs.extend(replay_inputs(player));
        }
        if let Some(rect) = layout.replay {
            focus.paint_chrome(
                &child,
                rect,
                &crate::focus::ChromeButton::Replay,
                accent,
                bar,
            );
        }

        if let Some(rect) = layout.seek {
            draw_seek_bar(
                &child, player, layout, rect, seek_drag, inputs, focus, text, accent, dim, bar,
            );
        }

        if let Some(rect) = layout.time {
            let color = if layout.can_seek { secondary } else { dim };
            painter_with(&child, layout.rect).text(
                Pos2::new(rect.left(), rect.center().y),
                Align2::LEFT_CENTER,
                &layout.time_text,
                FontId::new(SIZE_MONO, FontFamily::Monospace),
                color,
            );
        }

        if let Some(rect) = layout.volume {
            let muted = *volume <= 0.0;
            let icon = if muted {
                Icon::SpeakerSlash
            } else if *volume < 0.5 {
                Icon::SpeakerLow
            } else {
                Icon::SpeakerHigh
            };
            let tooltip = if muted {
                player.course().chrome_string("unmute")
            } else {
                player.course().chrome_string("mute")
            };
            if button(&child, rect, "mute", icon, text, true, None, tooltip) {
                toggle_volume(volume, last_volume, volume_changed);
            }
            focus.paint_chrome(&child, rect, &crate::focus::ChromeButton::Mute, accent, bar);
        }

        if let Some(rect) = layout.volume_slider {
            let sense = Sense::click_and_drag();
            let response = child
                .interact(rect, strip_id("volume"), sense)
                .on_hover_text(player.course().chrome_string("volume"));
            if let Some(pos) = response.interact_pointer_pos()
                && (response.is_pointer_button_down_on()
                    || response.dragged()
                    || response.clicked())
            {
                let value = slider_value(rect, pos.x, 0.0, 1.0, VOLUME_HANDLE_HOVER);
                set_volume_value(volume, last_volume, value, volume_changed);
            }
            let hovered = response.hovered() || response.is_pointer_button_down_on();
            draw_slider(
                &child,
                rect,
                *volume,
                hovered,
                text,
                accent,
                VOLUME_TRACK,
                VOLUME_HANDLE,
                VOLUME_HANDLE_HOVER,
            );
            focus.paint_chrome(
                &child,
                rect,
                &crate::focus::ChromeButton::Volume,
                accent,
                bar,
            );
        }

        draw_secondary_controls(
            &child,
            player,
            layout,
            drawer_open,
            show_nav,
            inputs,
            focus,
            captions_on,
            text,
            accent,
            secondary,
            bar,
        );
        draw_speed_and_fullscreen(
            &child,
            player,
            layout,
            speed_open,
            fullscreen_toggle,
            fullscreen_on,
            inputs,
            focus,
            text,
            accent,
            bar,
        );
    }
}

/// The speed button with its popup list, and the full screen toggle.
#[allow(clippy::too_many_arguments)]
fn draw_speed_and_fullscreen(
    ui: &Ui,
    player: &Player<'_>,
    layout: &StripLayout,
    speed_open: &mut bool,
    fullscreen_toggle: &mut bool,
    fullscreen_on: bool,
    inputs: &mut Vec<Input>,
    focus: &crate::focus::FocusManager,
    text: Color32,
    accent: Color32,
    bar: Color32,
) {
    let course = player.course();
    if let Some(rect) = layout.speed {
        let rate = player.speed();
        let label = format_rate(rate);
        let id = strip_id("speed");
        let resp = ui
            .interact(rect, id, Sense::click())
            .on_hover_text(course.chrome_string("playbackSpeed"));
        let painter = ui.painter();
        let radius = CornerRadius::same(8);
        if *speed_open {
            painter.rect_filled(rect, radius, accent);
        } else if resp.hovered() {
            painter.rect_filled(rect, radius, Color32::from_white_alpha(0x1A));
        }
        painter.text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::new(SIZE_BODY, FontFamily::Name("inter-bold".into())),
            if *speed_open { Color32::WHITE } else { text },
        );
        if resp.clicked() {
            resp.request_focus();
            *speed_open = !*speed_open;
        }
        focus.paint_chrome(ui, rect, &crate::focus::ChromeButton::Speed, accent, bar);
        if *speed_open {
            draw_speed_popup(ui, rect, rate, speed_open, inputs, text, accent, bar);
        }
    } else {
        *speed_open = false;
    }
    if let Some(rect) = layout.fullscreen {
        let (icon, tooltip) = if fullscreen_on {
            (Icon::CornersIn, course.chrome_string("fullScreen"))
        } else {
            (Icon::CornersOut, course.chrome_string("fullScreen"))
        };
        if button(ui, rect, "fullscreen", icon, text, true, None, tooltip) {
            *fullscreen_toggle = true;
        }
        focus.paint_chrome(
            ui,
            rect,
            &crate::focus::ChromeButton::Fullscreen,
            accent,
            bar,
        );
    }
}

/// "1×", "1.5×", "0.75×".
fn format_rate(rate: f32) -> String {
    format!("{rate}\u{00D7}")
}

#[allow(clippy::too_many_arguments)]
fn draw_speed_popup(
    ui: &Ui,
    anchor: Rect,
    current: f32,
    speed_open: &mut bool,
    inputs: &mut Vec<Input>,
    text: Color32,
    accent: Color32,
    bar: Color32,
) {
    let rates = coursecraft_logic::PLAYBACK_RATES;
    let height = SPEED_ROW * rates.len() as f32 + 8.0;
    let width = 72.0;
    let popup = Rect::from_min_size(
        Pos2::new(
            anchor.center().x - width * 0.5,
            (anchor.top() - 6.0 - height).max(0.0),
        ),
        vec2(width, height),
    );
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        Id::unique("coursecraft-speed-popup"),
    ));
    painter.rect_filled(popup, CornerRadius::same(10), bar);
    let mut chosen = None;
    // Slowest at the bottom, nearest the strip, so the list reads upward.
    for (index, rate) in rates.iter().enumerate() {
        let row = Rect::from_min_size(
            Pos2::new(
                popup.left() + 4.0,
                popup.bottom() - 4.0 - SPEED_ROW * (index as f32 + 1.0),
            ),
            vec2(width - 8.0, SPEED_ROW),
        );
        let resp = ui.interact(
            row,
            Id::unique(("coursecraft-speed-row", index)),
            Sense::click(),
        );
        let selected = (*rate - current).abs() < 1e-3;
        if selected {
            painter.rect_filled(row, CornerRadius::same(6), accent);
        } else if resp.hovered() {
            painter.rect_filled(row, CornerRadius::same(6), Color32::from_white_alpha(0x1A));
        }
        painter.text(
            row.center(),
            Align2::CENTER_CENTER,
            format_rate(*rate),
            FontId::new(SIZE_BODY, FontFamily::Name("inter".into())),
            if selected { Color32::WHITE } else { text },
        );
        if resp.clicked() {
            chosen = Some(*rate);
        }
    }
    if let Some(rate) = chosen {
        inputs.push(Input::SetSpeed { rate });
        *speed_open = false;
    } else if ui.input(|i| i.pointer.any_pressed() || i.key_pressed(egui::Key::Escape))
        && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
        && !popup.contains(pos)
        && !anchor.contains(pos)
    {
        *speed_open = false;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_secondary_controls(
    ui: &Ui,
    player: &Player<'_>,
    layout: &StripLayout,
    drawer_open: &mut bool,
    show_nav: bool,
    inputs: &mut Vec<Input>,
    focus: &crate::focus::FocusManager,
    captions_on: &mut bool,
    text: Color32,
    accent: Color32,
    secondary: Color32,
    bar: Color32,
) {
    let slide = player.current_slide();
    let (is_first, is_last) = slide.map_or((true, true), |slide| {
        nav_limits(
            player.play_order().iter().map(|s| s.id.as_str()),
            slide.id.as_str(),
        )
    });
    let started = player.state().started;
    let can_nav = show_nav && started && !player.lightbox_open();
    let can_menu = player.effective_player().show_menu && started;

    if let Some(rect) = layout.captions {
        let fill = (*captions_on).then_some(accent);
        let icon_color = if *captions_on { Color32::WHITE } else { text };
        if button(
            ui,
            rect,
            "captions",
            Icon::Subtitles,
            icon_color,
            true,
            fill,
            player.course().chrome_string("captionsTooltip"),
        ) {
            *captions_on = !*captions_on;
        }
        focus.paint_chrome(ui, rect, &crate::focus::ChromeButton::Captions, accent, bar);
    }
    if let Some(rect) = layout.outline
        && button(
            ui,
            rect,
            "outline",
            Icon::List,
            text,
            can_menu,
            (*drawer_open).then_some(accent),
            player.course().chrome_string("outlineTooltip"),
        )
        && can_menu
    {
        *drawer_open = !*drawer_open;
    }
    if let Some(rect) = layout.outline {
        focus.paint_chrome(ui, rect, &crate::focus::ChromeButton::Outline, accent, bar);
    }
    if let Some(rect) = layout.previous
        && button(
            ui,
            rect,
            "previous",
            Icon::CaretLeft,
            text,
            can_nav && !is_first,
            None,
            player.course().chrome_string("previousTooltip"),
        )
        && can_nav
        && !is_first
    {
        inputs.push(Input::Previous);
    }
    if let Some(rect) = layout.previous {
        focus.paint_chrome(ui, rect, &crate::focus::ChromeButton::Previous, accent, bar);
    }
    if let Some(rect) = layout.next {
        let complete = slide.is_some_and(|slide| {
            player.state().base.ended
                || (slide.duration_ms > 0 && player.state().base.ms >= slide.duration_ms)
        });
        let unlocked = player.can_next();
        let accent_fill = (complete && unlocked).then_some(accent);
        let icon_color = if complete && unlocked {
            Color32::WHITE
        } else {
            text
        };
        let tooltip = if unlocked {
            player.course().chrome_string("nextTooltip")
        } else {
            player.course().chrome_string("nextLockedTooltip")
        };
        if button(
            ui,
            rect,
            "next",
            Icon::CaretRight,
            icon_color,
            can_nav && !is_last && unlocked,
            accent_fill,
            tooltip,
        ) && can_nav
            && !is_last
            && unlocked
        {
            inputs.push(Input::Next);
        }
    }
    if let Some(rect) = layout.next {
        focus.paint_chrome(ui, rect, &crate::focus::ChromeButton::Next, accent, bar);
    }

    let painter = painter_with(ui, layout.rect);
    if let Some(rect) = layout.title {
        painter.text(
            Pos2::new(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            &layout.title_text,
            FontId::new(SIZE_BODY, FontFamily::Name("inter".into())),
            text,
        );
    }
    if let Some(rect) = layout.counter {
        painter.text(
            Pos2::new(rect.left(), rect.center().y),
            Align2::LEFT_CENTER,
            &layout.counter_text,
            FontId::new(SIZE_BODY, FontFamily::Name("inter".into())),
            secondary,
        );
    }
}

fn painter_with(ui: &Ui, clip: Rect) -> egui::Painter {
    ui.painter().with_clip_rect(clip)
}

#[allow(clippy::too_many_arguments)]
fn draw_seek_bar(
    ui: &Ui,
    player: &Player<'_>,
    layout: &StripLayout,
    rect: Rect,
    seek_drag: &mut Option<u32>,
    inputs: &mut Vec<Input>,
    focus: &crate::focus::FocusManager,
    text: Color32,
    accent: Color32,
    dim: Color32,
    background: Color32,
) {
    let enabled = layout.can_seek;
    let response = ui
        .interact(
            rect,
            strip_id("seek"),
            if enabled {
                Sense::click_and_drag()
            } else {
                Sense::hover()
            },
        )
        .on_hover_text(player.course().chrome_string("slidePosition"));
    let info = player.seek_info();
    if enabled {
        let interacting = response.is_pointer_button_down_on()
            || response.dragged()
            || response.drag_stopped()
            || response.clicked();
        if interacting && let Some(pos) = response.interact_pointer_pos() {
            *seek_drag = Some(slider_ms(rect, pos.x, info.duration_ms, SEEK_HANDLE_HOVER));
        }
        if response.clicked() || response.drag_stopped() {
            let value = seek_drag
                .take()
                .or_else(|| {
                    response
                        .interact_pointer_pos()
                        .map(|pos| slider_ms(rect, pos.x, info.duration_ms, SEEK_HANDLE_HOVER))
                })
                .unwrap_or(info.elapsed_ms.min(info.duration_ms));
            inputs.push(Input::PlayerSeek {
                ms: u64::from(value),
            });
        }
    } else {
        *seek_drag = None;
    }

    let shown = seek_drag.unwrap_or(info.elapsed_ms).min(info.duration_ms);
    let fraction = progress_fraction(shown, info.duration_ms);
    let hovered = enabled && (response.hovered() || seek_drag.is_some());
    draw_slider(
        ui,
        rect,
        fraction,
        hovered,
        if enabled { text } else { dim },
        if enabled {
            accent
        } else {
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 0x55)
        },
        SEEK_TRACK,
        SEEK_HANDLE,
        SEEK_HANDLE_HOVER,
    );
    focus.paint_chrome(
        ui,
        rect,
        &crate::focus::ChromeButton::Seek,
        accent,
        background,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_slider(
    ui: &Ui,
    rect: Rect,
    fraction: f32,
    hovered: bool,
    handle_color: Color32,
    fill_color: Color32,
    track_height: f32,
    handle: f32,
    handle_hover: f32,
) {
    if !valid_rect(rect) {
        return;
    }
    let fraction = if fraction.is_finite() {
        fraction.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let radius = handle_hover.max(handle);
    let track = Rect::from_center_size(
        rect.center(),
        vec2(
            (rect.width() - radius * 2.0).max(1.0),
            track_height.max(1.0),
        ),
    );
    let painter = painter_with(ui, rect);
    let track_color =
        Color32::from_rgba_unmultiplied(handle_color.r(), handle_color.g(), handle_color.b(), 0x3D);
    painter.rect_filled(track, CornerRadius::same(2), track_color);
    let fill = Rect::from_min_size(track.min, vec2(track.width() * fraction, track.height()));
    if fill.width() > 0.0 {
        painter.rect_filled(fill, CornerRadius::same(2), fill_color);
    }
    let center = Pos2::new(
        (track.left() + track.width() * fraction).clamp(rect.left(), rect.right()),
        rect.center().y,
    );
    let radius = if hovered { handle_hover } else { handle };
    painter.circle_filled(center, radius, handle_color);
}

fn slider_value(rect: Rect, x: f32, min: f32, max: f32, inset: f32) -> f32 {
    let left = rect.left() + inset;
    let right = rect.right() - inset;
    let width = right - left;
    if !width.is_finite() || width <= 0.0 || !x.is_finite() {
        return min;
    }
    let fraction = ((x - left) / width).clamp(0.0, 1.0);
    (min + (max - min) * fraction).clamp(min, max)
}

fn slider_ms(rect: Rect, x: f32, duration_ms: u32, inset: f32) -> u32 {
    let value = slider_value(rect, x, 0.0, duration_ms as f32, inset);
    if value.is_finite() && value >= 0.0 {
        value.round().min(duration_ms as f32) as u32
    } else {
        0
    }
}

fn set_volume_value(volume: &mut f32, last_volume: &mut f32, value: f32, changed: &mut bool) {
    let value = coursecraft_playback::sanitize_volume(value);
    if value > 0.0 {
        *last_volume = value;
    }
    if (value - *volume).abs() > f32::EPSILON {
        *volume = value;
        *changed = true;
    }
}

fn toggle_volume(volume: &mut f32, last_volume: &mut f32, changed: &mut bool) {
    if *volume > 0.0 {
        *last_volume = *volume;
        set_volume_value(volume, last_volume, 0.0, changed);
    } else {
        let restore = if *last_volume > 0.0 {
            *last_volume
        } else {
            1.0
        };
        set_volume_value(volume, last_volume, restore, changed);
    }
}

#[allow(clippy::too_many_arguments)]
fn button(
    ui: &Ui,
    rect: Rect,
    name: &'static str,
    icon: Icon,
    color: Color32,
    enabled: bool,
    fill: Option<Color32>,
    tooltip: &str,
) -> bool {
    let id = strip_id(name);
    let resp = ui.interact(
        rect,
        id,
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let resp = resp.on_hover_text(tooltip);
    let painter = ui.painter();
    let hovered = enabled && resp.hovered();
    let radius = CornerRadius::same(8);
    if let Some(fill) = fill {
        painter.rect_filled(rect, radius, fill);
    } else if hovered || (resp.is_pointer_button_down_on() && enabled) {
        painter.rect_filled(rect, radius, Color32::from_white_alpha(0x1A));
    }
    if resp.has_focus() {
        painter.rect_stroke(
            rect.shrink(1.0),
            radius,
            Stroke::new(2.0, Color32::from_rgb(0x4D, 0x9D, 0xFF)),
            StrokeKind::Inside,
        );
    }
    let icon_color = if enabled {
        color
    } else {
        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 0x55)
    };
    let font = FontId::new(ICON_TOOLBAR, FontFamily::Name("phosphor".into()));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        icon.codepoint().to_string(),
        font,
        icon_color,
    );
    if resp.clicked() {
        resp.request_focus();
        true
    } else {
        false
    }
}

fn draw_lightbox_close(
    ui: &Ui,
    player: &Player<'_>,
    rect: Rect,
    focus: &crate::focus::FocusManager,
) -> bool {
    if !valid_rect(rect) {
        return false;
    }
    let (bar, text, accent) = bar_colors(player);
    let resp = ui
        .interact(rect, strip_id("close-lightbox"), Sense::click())
        .on_hover_text(player.course().chrome_string("closeLightbox"));
    let painter = ui.painter();
    let radius = rect.width().min(rect.height()) * 0.5;
    let fill = if resp.hovered() { text } else { bar };
    let icon_color = if resp.hovered() { bar } else { text };
    painter.circle_filled(rect.center(), radius, fill);
    painter.circle_stroke(rect.center(), radius, Stroke::new(1.0, text));
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        Icon::X.codepoint().to_string(),
        FontId::new(ICON_TOOLBAR, FontFamily::Name("phosphor".into())),
        icon_color,
    );
    focus.paint_chrome(
        ui,
        rect,
        &crate::focus::ChromeButton::CloseLightbox,
        accent,
        bar,
    );
    if resp.clicked() {
        resp.request_focus();
        true
    } else {
        false
    }
}

fn draw_scrim(ui: &Ui, full: Rect, drawer: Rect, progress: f32) {
    let scrim = Rect::from_min_max(
        Pos2::new(drawer.right().min(full.right()), full.top()),
        full.max,
    );
    if !valid_rect(scrim) {
        return;
    }
    let alpha = (89.0 * progress.clamp(0.0, 1.0)).round() as u8;
    ui.painter().rect_filled(
        scrim,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(0, 0, 0, alpha),
    );
}

struct EndedLayout {
    card: Rect,
    heading: Rect,
    small: Rect,
}

fn draw_ended(
    ui: &Ui,
    rect: Rect,
    accent: Color32,
    course: &coursecraft_course::Course,
) -> Option<EndedLayout> {
    if !valid_rect(rect) {
        return None;
    }
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(
        rect,
        CornerRadius::ZERO,
        Color32::from_rgba_unmultiplied(0, 0, 0, 140),
    );

    let heading_galley = painter.layout_no_wrap(
        course.chrome_string("courseComplete").to_owned(),
        FontId::new(SIZE_HEADING, FontFamily::Name("inter-bold".into())),
        Color32::from_rgb(0xF2, 0xF2, 0xF5),
    );
    let small_color = Color32::from_rgba_unmultiplied(255, 255, 255, 179);
    let small_galley = painter.layout_no_wrap(
        course.chrome_string("closeWindow").to_owned(),
        FontId::new(SIZE_SMALL, FontFamily::Name("inter".into())),
        small_color,
    );

    let layout = ended_layout(
        rect,
        heading_galley.size(),
        small_galley.size(),
        ENDED_ICON,
        ENDED_PAD,
        ENDED_GAP,
        ENDED_CARD_MIN_W,
    )?;
    let radius = CornerRadius::same(RADIUS_XL as u8);
    painter.add(popover_shadow(ThemeMode::Dark).as_shape(layout.card, radius));
    painter.rect_filled(layout.card, radius, ENDED_CARD_FILL);

    let icon_rect = Rect::from_center_size(
        Pos2::new(
            layout.card.center().x,
            layout.card.top() + ENDED_PAD + ENDED_ICON * 0.5,
        ),
        vec2(ENDED_ICON, ENDED_ICON),
    );
    painter.text(
        icon_rect.center(),
        Align2::CENTER_CENTER,
        Icon::CheckCircle.codepoint().to_string(),
        FontId::new(ENDED_ICON, FontFamily::Name("phosphor".into())),
        accent,
    );
    painter.galley(
        layout.heading.min,
        heading_galley,
        Color32::from_rgb(0xF2, 0xF2, 0xF5),
    );
    painter.galley(layout.small.min, small_galley, small_color);
    Some(layout)
}

/// Lay out the completion card and its text inside the dimmed slide rect.
fn ended_layout(
    rect: Rect,
    heading_size: egui::Vec2,
    small_size: egui::Vec2,
    icon: f32,
    pad: f32,
    gap: f32,
    min_width: f32,
) -> Option<EndedLayout> {
    if !valid_rect(rect) {
        return None;
    }
    let content_w = heading_size.x.max(small_size.x).max(icon);
    let card_w = (content_w + pad * 2.0)
        .max(min_width)
        .min(rect.width().max(0.0));
    let card_h = pad + icon + gap + heading_size.y + gap + small_size.y + pad;
    let card = Rect::from_center_size(rect.center(), vec2(card_w, card_h));
    let mut y = card.top() + pad + icon + gap;
    let heading = Rect::from_min_size(
        Pos2::new(card.center().x - heading_size.x * 0.5, y),
        heading_size,
    );
    y += heading_size.y + gap;
    let small = Rect::from_min_size(
        Pos2::new(card.center().x - small_size.x * 0.5, y),
        small_size,
    );
    Some(EndedLayout {
        card,
        heading,
        small,
    })
}

fn drawer_rect(full: Rect, progress: f32) -> Rect {
    let width = DRAWER_WIDTH.min(full.width());
    let eased = ease_out(progress.clamp(0.0, 1.0));
    let x = full.left() - width + width * eased;
    Rect::from_min_size(Pos2::new(x, full.top()), vec2(width, full.height()))
}

fn ease_out(t: f32) -> f32 {
    let inverse = 1.0 - t;
    1.0 - inverse * inverse * inverse
}

#[allow(clippy::too_many_arguments)]
fn draw_drawer(
    ui: &mut Ui,
    player: &Player<'_>,
    rect: Rect,
    progress: f32,
    show_nav: bool,
    drawer_open: &mut bool,
    inputs: &mut Vec<Input>,
    focus: &crate::focus::FocusManager,
    chrome: &mut Vec<(crate::focus::ChromeButton, Rect)>,
    view: &mut DrawerView,
    focus_search: &mut bool,
) {
    let painter = ui.painter().with_clip_rect(rect);
    painter.rect_filled(rect, CornerRadius::ZERO, DRAWER_BG);
    painter.vline(
        rect.right(),
        rect.y_range(),
        Stroke::new(1.0, Color32::from_white_alpha(0x18)),
    );
    let area = rect.shrink2(vec2(DRAWER_PAD, DRAWER_PAD));
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(area)
            .id_salt("coursecraft-player-drawer"),
    );
    child.multiply_opacity(progress.clamp(0.0, 1.0));
    child.set_clip_rect(rect);
    let title_font = FontId::new(SIZE_TITLE, FontFamily::Name("inter-bold".into()));
    let (header, _) =
        child.allocate_exact_size(vec2(child.available_width(), DRAWER_CLOSE), Sense::hover());
    let close = Rect::from_min_size(
        Pos2::new(header.right() - DRAWER_CLOSE, header.top()),
        vec2(DRAWER_CLOSE, DRAWER_CLOSE),
    );
    let title_width = (close.left() - header.left() - 8.0).max(0.0);
    chrome.push((crate::focus::ChromeButton::DrawerClose, close));
    let mut job = egui::text::LayoutJob::single_section(
        player.display_text(&player.course().title),
        egui::TextFormat::simple(title_font, DRAWER_TEXT),
    );
    job.wrap = egui::text::TextWrapping::truncate_at_width(title_width);
    let galley = child.painter().layout_job(job);
    child.painter().galley(
        Pos2::new(header.left(), header.center().y - galley.size().y * 0.5),
        galley,
        DRAWER_TEXT,
    );
    if button(
        &child,
        close,
        "drawer-close",
        Icon::X,
        DRAWER_TEXT,
        true,
        None,
        player.course().chrome_string("closeOutlineTooltip"),
    ) {
        *drawer_open = false;
    }
    focus.paint_chrome(
        &child,
        close,
        &crate::focus::ChromeButton::DrawerClose,
        parse_color(&player.course().theme.accent).unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB)),
        DRAWER_BG,
    );
    child.add_space(6.0);
    let accent =
        parse_color(&player.course().theme.accent).unwrap_or(Color32::from_rgb(0x2F, 0x6F, 0xEB));
    let body_font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let small_font = FontId::new(11.0, FontFamily::Name("inter-bold".into()));
    let icon_font = FontId::new(14.0, FontFamily::Name("phosphor".into()));
    let started = player.state().started;
    let can_navigate = show_nav && started;
    let mut go_to = None;
    let tabs = available_tabs(player.course());
    let tab = view.effective_tab(player.course());
    if tabs.len() > 1 {
        draw_tab_row(&mut child, player, &tabs, tab, view, focus, chrome, accent);
        child.add_space(6.0);
    }
    if tab != DrawerTab::Menu {
        draw_content_tab(
            &mut child,
            player,
            tab,
            view,
            focus,
            chrome,
            inputs,
            focus_search,
            accent,
        );
        return;
    }

    // The outline lists drawn questions, not draw slides: expand each scene
    // through the player's play order, consuming bank picks in course order.
    let outline: Vec<(&coursecraft_course::Scene, Vec<&coursecraft_course::Slide>)> = {
        let play_order = player.play_order();
        let mut cursor = 0usize;
        let mut bank_taken: std::collections::BTreeMap<String, usize> =
            std::collections::BTreeMap::new();
        let mut outline = Vec::new();
        for scene in &player.course().scenes {
            let mut expanded = Vec::new();
            for slide in &scene.slides {
                if let Some(draw) = &slide.draw {
                    let bank_size = player
                        .course()
                        .bank(&draw.bank)
                        .map_or(0, |bank| bank.slides.len());
                    let already = bank_taken.get(&draw.bank).copied().unwrap_or(0);
                    let taken_now = (draw.count as usize).min(bank_size.saturating_sub(already));
                    bank_taken.insert(draw.bank.clone(), already.saturating_add(taken_now));
                    for _ in 0..taken_now {
                        if let Some(picked) = play_order.get(cursor) {
                            expanded.push(*picked);
                            cursor = cursor.saturating_add(1);
                        }
                    }
                } else if play_order
                    .get(cursor)
                    .is_some_and(|current| current.id == slide.id)
                {
                    expanded.push(slide);
                    cursor = cursor.saturating_add(1);
                }
            }
            outline.push((scene, expanded));
        }
        outline
    };

    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(&mut child, |ui| {
            for (scene, expanded) in &outline {
                let scene_title = player.display_text(&scene.title);
                ui.add_space(8.0);
                ui.label(
                    RichText::new(scene_title)
                        .font(small_font.clone())
                        .color(DRAWER_SECONDARY),
                );
                ui.add_space(2.0);
                for slide in expanded {
                    let current = player.current_slide().is_some_and(|s| s.id == slide.id);
                    let visited = player.state().visited.contains(&slide.id);
                    let reachable = current || player.can_go_to(&slide.id);
                    let (row, resp) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                    chrome.push((crate::focus::ChromeButton::DrawerRow(slide.id.clone()), row));
                    if current {
                        painter.rect_filled(
                            row,
                            CornerRadius::same(6),
                            Color32::from_rgba_unmultiplied(
                                accent.r(),
                                accent.g(),
                                accent.b(),
                                0x44,
                            ),
                        );
                    } else if resp.hovered() && reachable {
                        painter.rect_filled(
                            row,
                            CornerRadius::same(6),
                            Color32::from_white_alpha(0x0E),
                        );
                    }
                    focus.paint_chrome(
                        ui,
                        row,
                        &crate::focus::ChromeButton::DrawerRow(slide.id.clone()),
                        accent,
                        DRAWER_BG,
                    );
                    let marker = Pos2::new(row.left() + 12.0, row.center().y);
                    if !reachable {
                        painter.text(
                            marker,
                            Align2::CENTER_CENTER,
                            Icon::Lock.codepoint().to_string(),
                            icon_font.clone(),
                            DRAWER_TERTIARY,
                        );
                    } else if visited {
                        painter.text(
                            marker,
                            Align2::CENTER_CENTER,
                            Icon::Check.codepoint().to_string(),
                            icon_font.clone(),
                            accent,
                        );
                    } else if let Some(index) = player.slide_position(&slide.id) {
                        painter.text(
                            marker,
                            Align2::CENTER_CENTER,
                            (index + 1).to_string(),
                            small_font.clone(),
                            DRAWER_TERTIARY,
                        );
                    }
                    let title = player.display_text(&slide.title);
                    painter.text(
                        Pos2::new(row.left() + 30.0, row.center().y),
                        Align2::LEFT_CENTER,
                        title,
                        body_font.clone(),
                        if current {
                            DRAWER_TEXT
                        } else if reachable {
                            DRAWER_SECONDARY
                        } else {
                            DRAWER_TERTIARY
                        },
                    );
                    if resp.clicked() && !current && reachable {
                        go_to = Some(slide.id.clone());
                    }
                }
            }
        });

    if let Some(slide) = go_to
        && can_navigate
    {
        inputs.push(Input::GoTo { slide });
    }
}

/// The row of drawer tab buttons, one per available tab.
#[allow(clippy::too_many_arguments)]
fn draw_tab_row(
    ui: &mut Ui,
    player: &Player<'_>,
    tabs: &[DrawerTab],
    current: DrawerTab,
    view: &mut DrawerView,
    focus: &crate::focus::FocusManager,
    chrome: &mut Vec<(crate::focus::ChromeButton, Rect)>,
    accent: Color32,
) {
    let font = FontId::new(SIZE_BODY, FontFamily::Name("inter-bold".into()));
    let width = ui.available_width();
    let gap = 4.0;
    let each =
        ((width - gap * tabs.len().saturating_sub(1) as f32) / tabs.len().max(1) as f32).max(24.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = gap;
        for tab in tabs {
            let (rect, resp) = ui.allocate_exact_size(vec2(each, 30.0), Sense::click());
            let kind = crate::focus::ChromeButton::DrawerTab(*tab);
            chrome.push((kind.clone(), rect));
            let selected = *tab == current;
            if selected {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(6), accent);
            } else if resp.hovered() {
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(6),
                    Color32::from_white_alpha(0x1A),
                );
            }
            focus.paint_chrome(ui, rect, &kind, accent, DRAWER_BG);
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                player.course().chrome_string(tab.label_key()),
                font.clone(),
                if selected {
                    Color32::WHITE
                } else {
                    DRAWER_SECONDARY
                },
            );
            if resp.clicked() {
                view.tab = *tab;
                view.selected = None;
            }
        }
    });
}

fn glossary_search_id() -> Id {
    Id::unique("coursecraft-glossary-search")
}

/// One tappable row in a drawer list: highlight, focus ring and chrome rect.
fn drawer_item(
    ui: &mut Ui,
    kind: &crate::focus::ChromeButton,
    height: f32,
    selected: bool,
    focus: &crate::focus::FocusManager,
    chrome: &mut Vec<(crate::focus::ChromeButton, Rect)>,
    accent: Color32,
) -> (Rect, egui::Response) {
    let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
    chrome.push((kind.clone(), row));
    if selected {
        ui.painter().rect_filled(
            row,
            CornerRadius::same(6),
            Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 0x44),
        );
    } else if resp.hovered() {
        ui.painter()
            .rect_filled(row, CornerRadius::same(6), Color32::from_white_alpha(0x0E));
    }
    focus.paint_chrome(ui, row, kind, accent, DRAWER_BG);
    (row, resp)
}

/// The Resources, Glossary and Transcript tabs.
#[allow(clippy::too_many_arguments)]
fn draw_content_tab(
    ui: &mut Ui,
    player: &Player<'_>,
    tab: DrawerTab,
    view: &mut DrawerView,
    focus: &crate::focus::FocusManager,
    chrome: &mut Vec<(crate::focus::ChromeButton, Rect)>,
    inputs: &mut Vec<Input>,
    focus_search: &mut bool,
    accent: Color32,
) {
    let course = player.course();
    let body_font = FontId::new(SIZE_BODY, FontFamily::Name("inter".into()));
    let bold_font = FontId::new(SIZE_BODY, FontFamily::Name("inter-bold".into()));
    let icon_font = FontId::new(14.0, FontFamily::Name("phosphor".into()));
    match tab {
        DrawerTab::Menu => {}
        DrawerTab::Resources => {
            ScrollArea::vertical()
                .id_salt("drawer-resources")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for resource in &course.resources {
                        let kind = crate::focus::ChromeButton::ResourceRow(resource.id.clone());
                        let (row, resp) =
                            drawer_item(ui, &kind, 34.0, false, focus, chrome, accent);
                        ui.painter().text(
                            Pos2::new(row.left() + 8.0, row.center().y),
                            Align2::LEFT_CENTER,
                            &resource.title,
                            body_font.clone(),
                            DRAWER_TEXT,
                        );
                        ui.painter().text(
                            Pos2::new(row.right() - 12.0, row.center().y),
                            Align2::CENTER_CENTER,
                            Icon::ArrowSquareOut.codepoint().to_string(),
                            icon_font.clone(),
                            DRAWER_SECONDARY,
                        );
                        if resp.clicked() {
                            inputs.push(Input::OpenResource {
                                resource: resource.id.clone(),
                            });
                        }
                    }
                });
        }
        DrawerTab::Glossary => {
            let search_id = glossary_search_id();
            if std::mem::take(focus_search) {
                ui.memory_mut(|memory| memory.request_focus(search_id));
            }
            let search = ui.add(
                egui::TextEdit::singleline(&mut view.query)
                    .id(search_id)
                    .hint_text(course.chrome_string("glossarySearch"))
                    .desired_width(f32::INFINITY),
            );
            chrome.push((crate::focus::ChromeButton::GlossarySearch, search.rect));
            focus.paint_chrome(
                ui,
                search.rect,
                &crate::focus::ChromeButton::GlossarySearch,
                accent,
                DRAWER_BG,
            );
            ui.add_space(6.0);
            let entries = visible_glossary(course, &view.query);
            ScrollArea::vertical()
                .id_salt("drawer-glossary")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    for entry in &entries {
                        let kind = crate::focus::ChromeButton::GlossaryTerm(entry.id.clone());
                        let open = view.selected.as_deref() == Some(entry.id.as_str());
                        let (row, resp) = drawer_item(ui, &kind, 30.0, open, focus, chrome, accent);
                        ui.painter().text(
                            Pos2::new(row.left() + 8.0, row.center().y),
                            Align2::LEFT_CENTER,
                            &entry.term,
                            body_font.clone(),
                            DRAWER_TEXT,
                        );
                        if resp.clicked() {
                            view.selected = Some(entry.id.clone());
                        }
                    }
                    if let Some(entry) = entries
                        .iter()
                        .find(|entry| view.selected.as_deref() == Some(entry.id.as_str()))
                    {
                        ui.add_space(8.0);
                        ui.separator();
                        ui.label(
                            RichText::new(&entry.term)
                                .font(bold_font.clone())
                                .color(DRAWER_TEXT),
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(&entry.definition)
                                    .font(body_font.clone())
                                    .color(DRAWER_SECONDARY),
                            )
                            .wrap(),
                        );
                    }
                });
        }
        DrawerTab::Transcript => {
            let notes = player
                .current_slide()
                .map(|slide| slide.notes.trim().to_owned())
                .unwrap_or_default();
            let text = if notes.is_empty() {
                course.chrome_string("transcriptEmpty").to_owned()
            } else {
                notes
            };
            let area = ui.available_rect_before_wrap();
            chrome.push((crate::focus::ChromeButton::TranscriptText, area));
            focus.paint_chrome(
                ui,
                area,
                &crate::focus::ChromeButton::TranscriptText,
                accent,
                DRAWER_BG,
            );
            ScrollArea::vertical()
                .id_salt("drawer-transcript")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(text)
                                .font(body_font.clone())
                                .color(DRAWER_TEXT),
                        )
                        .wrap(),
                    );
                });
        }
    }
}

/// Paint the loading screen: course title over a thin indeterminate line.
pub fn show_loading(ui: &mut Ui, title: &str) {
    let default = coursecraft_course::CHROME_STRINGS
        .iter()
        .find_map(|&(key, text)| (key == "loading").then_some(text))
        .unwrap_or("Loading");
    show_loading_text(ui, title, default);
}

/// Paint the loading screen with the course's translated fallback label.
pub fn show_loading_text(ui: &mut Ui, title: &str, loading: &str) {
    let requested = ui.available_size();
    if !requested.x.is_finite()
        || !requested.y.is_finite()
        || requested.x <= 0.0
        || requested.y <= 0.0
    {
        return;
    }
    let (full, _) = ui.allocate_exact_size(requested, Sense::hover());
    if !valid_rect(full) {
        return;
    }
    let painter = ui.painter();
    painter.rect_filled(full, CornerRadius::ZERO, PLAYER_GROUND);
    let center = full.center();
    let font = FontId::new(SIZE_TITLE, FontFamily::Name("inter-bold".into()));
    let label = if title.is_empty() { loading } else { title };
    painter.text(
        Pos2::new(center.x, center.y - 18.0),
        Align2::CENTER_CENTER,
        label,
        font,
        Color32::from_rgb(0xF2, 0xF2, 0xF5),
    );
    let line = Rect::from_center_size(Pos2::new(center.x, center.y + 14.0), vec2(180.0, 2.0));
    painter.rect_filled(line, CornerRadius::same(1), Color32::from_white_alpha(0x26));
    let time = ui.input(|i| i.time);
    let phase = ((time * 0.8) % 1.0) as f32;
    let segment = line.width() * 0.34;
    let x = line.left() + (line.width() - segment) * phase;
    let active = Rect::from_min_size(Pos2::new(x, line.top()), vec2(segment, line.height()));
    painter.rect_filled(
        active,
        CornerRadius::same(1),
        Color32::from_rgb(0x4D, 0x9D, 0xFF),
    );
    ui.ctx().request_repaint();
}

#[cfg(test)]
mod tests;
