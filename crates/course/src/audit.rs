//! Automated accessibility checks.
//!
//! This module measures only what a tool can measure: names, sizes, captions,
//! focus order and text contrast against a background it can resolve. It says
//! nothing about whether a course is accessible; the manual checks in
//! [`MANUAL_CHECKS`] are always part of the report.
//!
//! Findings are in play order: course-wide findings first, then each slide in
//! play order, then each object on the base layer and every layer bottom to top.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::{Color, Course, Event, Object, ObjectKind, Rect, Slide, Theme};

/// How serious an audit finding is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditSeverity {
    Error,
    Warning,
    Note,
}

/// One automated finding. `check` is a stable id; `criterion` names the WCAG
/// success criterion or `"Best practice"`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditFinding {
    pub severity: AuditSeverity,
    pub check: &'static str,
    pub criterion: &'static str,
    pub slide: Option<String>,
    pub object: Option<String>,
    pub message: String,
    pub fix: String,
}

/// The automated findings plus the checks only a person can do.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AuditReport {
    pub findings: Vec<AuditFinding>,
    pub manual_checks: &'static [&'static str],
}

/// Things this tool cannot judge, always reported with the findings.
pub const MANUAL_CHECKS: &[&str] = &[
    "Check that the reading order makes sense when the slide is read aloud.",
    "Check that alt text and descriptions say what matters on each slide.",
    "Check that captions are accurate and include important sounds.",
    "Check that nothing relies on colour alone.",
    "Check that instructions do not rely on shape or position alone.",
    "Check that timed content can be paused or extended.",
    "Do a real screen reader and keyboard pass before publishing.",
];

const COURSE_TITLE: &str = "WCAG 2.4.2";
const SLIDE_TITLE: &str = "WCAG 2.4.2";
const ALT_TEXT: &str = "WCAG 1.1.1";
const CONTROL_NAME: &str = "WCAG 4.1.2";
const CONTRAST: &str = "WCAG 1.4.3";
const TEXT_SIZE: &str = "Best practice";
const TARGET_SIZE: &str = "WCAG 2.5.8 (WCAG 2.2)";
const VIDEO_CAPTIONS: &str = "WCAG 1.2.2";
const AUDIO_CAPTIONS: &str = "WCAG 1.2.1";
const AUTOPLAY_CONTROL: &str = "WCAG 1.4.2";
const FOCUS_ORDER: &str = "WCAG 2.4.3";
const DECORATIVE_INTERACTIVE: &str = "WCAG 4.1.2";

/// Minimum slide-pixel size for body text (12 pt).
pub const MIN_TEXT_PX: f32 = 16.0;
/// Minimum slide-pixel size for keyboard and pointer targets.
pub const MIN_TARGET_PX: f32 = 24.0;
/// WCAG large text starts at 18 pt (24 px).
pub const LARGE_TEXT_PX: f32 = 24.0;
/// WCAG large bold text starts at 14 pt (about 18.66 px).
pub const LARGE_BOLD_PX: f32 = 18.66;
/// WCAG AA minimum for body text.
pub const NORMAL_CONTRAST: f64 = 4.5;
/// WCAG AA minimum for large text.
pub const LARGE_CONTRAST: f64 = 3.0;

/// Ratios this close are treated as equal, so a true 4.5:1 pair passes despite
/// floating-point rounding.
const CONTRAST_EPSILON: f64 = 1e-4;

const DEFAULT_TEXT: Rgba = Rgba {
    r: 0x1F,
    g: 0x1F,
    b: 0x1F,
    a: 255,
};
const WHITE: Rgba = Rgba {
    r: 255,
    g: 255,
    b: 255,
    a: 255,
};
const DEFAULT_BUTTON: Rgba = Rgba {
    r: 0x2F,
    g: 0x6F,
    b: 0xEB,
    a: 255,
};

impl Course {
    /// Run the automated accessibility checks. Never panics, including on
    /// non-finite frames and sizes.
    #[must_use]
    pub fn audit(&self) -> AuditReport {
        let mut audit = Audit {
            course: self,
            findings: Vec::new(),
        };
        audit.run();
        AuditReport {
            findings: audit.findings,
            manual_checks: MANUAL_CHECKS,
        }
    }
}

struct Audit<'a> {
    course: &'a Course,
    findings: Vec<AuditFinding>,
}

impl Audit<'_> {
    fn run(&mut self) {
        if self.course.title.trim().is_empty() {
            self.push(
                AuditSeverity::Error,
                "course-title",
                COURSE_TITLE,
                None,
                None,
                "The course title is empty.",
                "Write a title that names the course.",
            );
        }
        for slide in self.course.slides() {
            self.check_slide(slide);
        }
        for master in &self.course.masters {
            self.check_slide(master);
        }
        for bank in &self.course.banks {
            for slide in &bank.slides {
                self.check_slide(slide);
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        severity: AuditSeverity,
        check: &'static str,
        criterion: &'static str,
        slide: Option<&Slide>,
        object: Option<&Object>,
        message: impl Into<String>,
        fix: impl Into<String>,
    ) {
        let mut message = message.into();
        if let Some(slide) = slide
            && self.course.master(&slide.id).is_some()
        {
            message = format!("Master {:?}: {message}", slide.title);
        }
        self.findings.push(AuditFinding {
            severity,
            check,
            criterion,
            slide: slide.map(|s| s.id.clone()),
            object: object.map(|o| o.id.clone()),
            message,
            fix: fix.into(),
        });
    }

    fn check_slide(&mut self, slide: &Slide) {
        if slide.title.trim().is_empty() {
            self.push(
                AuditSeverity::Error,
                "slide-title",
                SLIDE_TITLE,
                Some(slide),
                None,
                "The slide title is empty.",
                "Write a title that describes the slide.",
            );
        }
        for (index, object) in slide.objects.iter().enumerate() {
            let below: Vec<&Object> = slide.objects.iter().take(index).collect();
            self.check_object(slide, object, &below);
        }
        for layer in &slide.layers {
            for (index, object) in layer.objects.iter().enumerate() {
                let mut below: Vec<&Object> = slide.objects.iter().collect();
                below.extend(layer.objects.iter().take(index));
                self.check_object(slide, object, &below);
            }
        }
        self.check_question_text(slide);
        self.check_focus_order(slide);
    }

    fn check_object(&mut self, slide: &Slide, object: &Object, below: &[&Object]) {
        if object.decorative {
            if is_interactive(slide, object) {
                self.push(
                    AuditSeverity::Error,
                    "decorative-interactive",
                    DECORATIVE_INTERACTIVE,
                    Some(slide),
                    Some(object),
                    "The object is marked decorative but takes pointer input.",
                    "Remove `decorative` and give the object a label, or stop it taking clicks.",
                );
            }
            return;
        }
        self.check_alt_text(slide, object);
        self.check_control_name(slide, object);
        match &object.kind {
            ObjectKind::Text { .. } => self.check_text(slide, object, below),
            ObjectKind::Shape { text: Some(_), .. } => {
                self.check_shape_text(slide, object, below);
            }
            ObjectKind::Button { .. } => self.check_button(slide, object, below),
            ObjectKind::Audio {
                autoplay, captions, ..
            } => {
                self.check_audio(slide, object, *autoplay, captions.as_deref());
            }
            ObjectKind::Video {
                autoplay, captions, ..
            } => {
                self.check_video(slide, object, *autoplay, captions.as_deref());
            }
            // A path has no text of its own. Its `description` is its only name,
            // and an unnamed path is decorative by default, so there is nothing
            // automated to measure.
            ObjectKind::Path { .. } => {}
            _ => {}
        }
        self.check_target_size(slide, object);
    }

    fn check_alt_text(&mut self, slide: &Slide, object: &Object) {
        let ObjectKind::Image { alt, .. } = &object.kind else {
            return;
        };
        if alt.trim().is_empty() && !has_description(object) {
            self.push(
                AuditSeverity::Error,
                "alt-text",
                ALT_TEXT,
                Some(slide),
                Some(object),
                "The image has no alt text.",
                "Add alt text that says what the image shows, or mark it decorative.",
            );
        }
    }

    fn check_control_name(&mut self, slide: &Slide, object: &Object) {
        if !is_interactive(slide, object) || has_label(object) || has_description(object) {
            return;
        }
        self.push(
            AuditSeverity::Error,
            "control-name",
            CONTROL_NAME,
            Some(slide),
            Some(object),
            "The interactive object has no label.",
            "Add a label or description that names what it does, or mark it decorative.",
        );
    }

    fn check_text(&mut self, slide: &Slide, object: &Object, below: &[&Object]) {
        let frame = Frame::from_rect(&object.frame);
        for run in resolved_runs(object, &self.course.theme) {
            if run.text.trim().is_empty() {
                continue;
            }
            self.check_text_size(slide, object, &run);
            if let Some(frame) = frame {
                self.check_text_contrast(slide, object, frame, &run, below);
            }
        }
    }

    fn check_text_size(&mut self, slide: &Slide, object: &Object, run: &ResolvedRun<'_>) {
        if run.size < MIN_TEXT_PX {
            self.push(
                AuditSeverity::Warning,
                "text-size",
                TEXT_SIZE,
                Some(slide),
                Some(object),
                format!(
                    "Text {:?} is {:.0} slide px, under 16 px (12 pt).",
                    snippet(run.text),
                    run.size
                ),
                "Raise it to at least 16 slide px. The player may scale text, so treat this as a floor.",
            );
        }
    }

    fn check_text_contrast(
        &mut self,
        slide: &Slide,
        object: &Object,
        frame: Frame,
        run: &ResolvedRun<'_>,
        below: &[&Object],
    ) {
        if frame.w <= 0.0 || frame.h <= 0.0 {
            return;
        }
        let subject = format!("Text {:?}", snippet(run.text));
        match measure_background(self.course, slide, frame, below) {
            Background::Unknown => self.push_contrast_unknown(slide, object, &subject),
            Background::Known(background) => self.push_contrast(
                slide,
                Some(object),
                ContrastSpec {
                    subject: &subject,
                    foreground: run.color,
                    background,
                    size: run.size,
                    bold: run.bold,
                },
            ),
        }
    }

    fn check_shape_text(&mut self, slide: &Slide, object: &Object, below: &[&Object]) {
        let ObjectKind::Shape { fill, .. } = &object.kind else {
            return;
        };
        let frame = Frame::from_rect(&object.frame);
        let parsed_fill = parse_color(fill);
        for run in resolved_runs(object, &self.course.theme) {
            if run.text.trim().is_empty() {
                continue;
            }
            self.check_text_size(slide, object, &run);
            let Some(frame) = frame else {
                continue;
            };
            if frame.w <= 0.0 || frame.h <= 0.0 {
                continue;
            }
            let background = match parsed_fill {
                // No fill, or a fill that is not a usable colour: measure what
                // is behind the shape exactly as text objects do.
                None => measure_background(self.course, slide, frame, below),
                Some(fill) if fill.a == 0 => measure_background(self.course, slide, frame, below),
                Some(fill) if fill.a == 255 => Background::Known(fill),
                Some(fill) => match measure_background(self.course, slide, frame, below) {
                    Background::Known(background) => Background::Known(fill.over(background)),
                    Background::Unknown => Background::Unknown,
                },
            };
            let subject = format!("Shape text {:?}", snippet(run.text));
            match background {
                Background::Unknown => self.push_contrast_unknown(slide, object, &subject),
                Background::Known(background) => self.push_contrast(
                    slide,
                    Some(object),
                    ContrastSpec {
                        subject: &subject,
                        foreground: run.color,
                        background,
                        size: run.size,
                        bold: run.bold,
                    },
                ),
            }
        }
    }

    fn check_button(&mut self, slide: &Slide, object: &Object, below: &[&Object]) {
        let ObjectKind::Button { label, fill, .. } = &object.kind else {
            return;
        };
        if label.trim().is_empty() {
            return;
        }
        let Some(frame) = Frame::from_rect(&object.frame) else {
            return;
        };
        let fill = parse_color(fill)
            .or_else(|| parse_color(&self.course.theme.accent))
            .unwrap_or(DEFAULT_BUTTON);
        let background = if fill.a == 255 {
            Background::Known(fill)
        } else {
            match measure_background(self.course, slide, frame, below) {
                Background::Known(background) => Background::Known(fill.over(background)),
                Background::Unknown => Background::Unknown,
            }
        };
        let subject = format!("Button label {:?}", snippet(label));
        match background {
            Background::Unknown => self.push_contrast_unknown(slide, object, &subject),
            Background::Known(background) => {
                self.push_contrast(
                    slide,
                    Some(object),
                    ContrastSpec {
                        subject: &subject,
                        foreground: WHITE,
                        background,
                        size: 20.0,
                        bold: false,
                    },
                );
            }
        }
    }

    fn check_audio(
        &mut self,
        slide: &Slide,
        object: &Object,
        autoplay: bool,
        captions: Option<&str>,
    ) {
        if captions.is_none_or(|c| c.trim().is_empty()) {
            self.push(
                AuditSeverity::Warning,
                "audio-captions",
                AUDIO_CAPTIONS,
                Some(slide),
                Some(object),
                "The audio has no captions.",
                "Add a WebVTT captions file, or make a transcript available elsewhere.",
            );
        }
        self.check_autoplay(slide, object, autoplay, "audio");
    }

    fn check_video(
        &mut self,
        slide: &Slide,
        object: &Object,
        autoplay: bool,
        captions: Option<&str>,
    ) {
        if captions.is_none_or(|c| c.trim().is_empty()) {
            self.push(
                AuditSeverity::Error,
                "video-captions",
                VIDEO_CAPTIONS,
                Some(slide),
                Some(object),
                "The video has no captions.",
                "Add a WebVTT captions file with `object.set_captions`.",
            );
        }
        self.check_autoplay(slide, object, autoplay, "video");
    }

    fn check_autoplay(&mut self, slide: &Slide, object: &Object, autoplay: bool, kind: &str) {
        if !autoplay || has_pause_or_stop(slide, &object.id) {
            return;
        }
        self.push(
            AuditSeverity::Warning,
            "autoplay-control",
            AUTOPLAY_CONTROL,
            Some(slide),
            Some(object),
            format!("The {kind} plays automatically and no trigger pauses or stops it."),
            "Add a trigger that pauses or stops it, or turn off autoplay.",
        );
    }

    fn check_target_size(&mut self, slide: &Slide, object: &Object) {
        if !is_interactive(slide, object) {
            return;
        }
        let Some(frame) = Frame::from_rect(&object.frame) else {
            return;
        };
        if frame.w < MIN_TARGET_PX || frame.h < MIN_TARGET_PX {
            self.push(
                AuditSeverity::Warning,
                "target-size",
                TARGET_SIZE,
                Some(slide),
                Some(object),
                format!(
                    "The interactive area is {:.0} by {:.0} slide px, under 24 by 24 px.",
                    frame.w, frame.h
                ),
                "Make the clickable area at least 24 by 24 slide px, or move the action to a larger control.",
            );
        }
    }

    fn check_question_text(&mut self, slide: &Slide) {
        let Some(question) = &slide.question else {
            return;
        };
        let theme = &self.course.theme;
        let text = resolve_theme_text(theme);
        let background = parse_color(self.course.slide_background(slide))
            .unwrap_or(WHITE)
            .over(WHITE);
        if !question.prompt.trim().is_empty() {
            let subject = format!("Question prompt {:?}", snippet(&question.prompt));
            self.push_contrast(
                slide,
                None,
                ContrastSpec {
                    subject: &subject,
                    foreground: text,
                    background,
                    size: 28.0,
                    bold: true,
                },
            );
        }
        if !matches!(
            question.kind,
            crate::QuestionKind::MultipleChoice
                | crate::QuestionKind::MultipleResponse
                | crate::QuestionKind::TrueFalse
        ) {
            return;
        }
        for choice in &question.choices {
            if choice.text.trim().is_empty() {
                continue;
            }
            let subject = format!("Choice {:?}", snippet(&choice.text));
            self.push_contrast(
                slide,
                None,
                ContrastSpec {
                    subject: &subject,
                    foreground: text,
                    background,
                    size: 22.0,
                    bold: false,
                },
            );
        }
    }

    fn check_focus_order(&mut self, slide: &Slide) {
        let mut groups: BTreeMap<u32, Vec<&Object>> = BTreeMap::new();
        for object in slide
            .objects
            .iter()
            .chain(slide.layers.iter().flat_map(|l| l.objects.iter()))
        {
            if object.decorative {
                continue;
            }
            if let Some(order) = object.focus_order {
                groups.entry(order).or_default().push(object);
            }
        }
        for (order, objects) in groups {
            if objects.len() < 2 {
                continue;
            }
            let ids: Vec<&str> = objects.iter().map(|o| o.id.as_str()).collect();
            self.push(
                AuditSeverity::Warning,
                "focus-order",
                FOCUS_ORDER,
                Some(slide),
                None,
                format!(
                    "Objects {} share focus_order {order}.",
                    ids.iter()
                        .map(|id| format!("{id:?}"))
                        .collect::<Vec<_>>()
                        .join(" and ")
                ),
                "Give each object its own focus_order, or remove it to keep the default reading order.",
            );
        }
    }

    fn push_contrast(&mut self, slide: &Slide, object: Option<&Object>, spec: ContrastSpec<'_>) {
        let large = is_large_text(spec.size, spec.bold);
        let foreground = spec.foreground.over(spec.background);
        let ratio = contrast_ratio(foreground, spec.background);
        if contrast_passes(ratio, large) {
            return;
        }
        let minimum = if large {
            LARGE_CONTRAST
        } else {
            NORMAL_CONTRAST
        };
        self.push(
            AuditSeverity::Error,
            "contrast",
            CONTRAST,
            Some(slide),
            object,
            format!(
                "{} has {ratio:.1}:1 contrast against its background; {minimum:.1}:1 is the minimum here.",
                spec.subject
            ),
            "Use a darker or lighter colour for the text or its background. The player may scale text, so check the slide at the size it will be shown.",
        );
    }

    fn push_contrast_unknown(&mut self, slide: &Slide, object: &Object, subject: &str) {
        self.push(
            AuditSeverity::Note,
            "contrast-unknown",
            CONTRAST,
            Some(slide),
            Some(object),
            format!("{subject} sits over an image, video or partly covering shape, so its background cannot be measured."),
            "Check the text where it appears and keep enough contrast on every part of the background. The player may scale text.",
        );
    }
}

/// WCAG AA threshold check with a small tolerance for floating-point equality.
#[must_use]
pub fn contrast_passes(ratio: f64, large: bool) -> bool {
    let minimum = if large {
        LARGE_CONTRAST
    } else {
        NORMAL_CONTRAST
    };
    ratio + CONTRAST_EPSILON >= minimum
}

/// WCAG large text: 18 pt (24 px), or 14 pt (about 18.66 px) when bold.
#[must_use]
pub fn is_large_text(size: f32, bold: bool) -> bool {
    size >= LARGE_TEXT_PX || (bold && size >= LARGE_BOLD_PX)
}

// ------------------------------------------------------------------ colours

/// Unmultiplied sRGB colour with 8-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// Parse `#RRGGBB` or `#RRGGBBAA`. Any other format returns `None`.
#[must_use]
pub fn parse_color(color: &Color) -> Option<Rgba> {
    let hex = color.0.strip_prefix('#')?;
    if !hex.is_ascii() {
        return None;
    }
    let channel = |start: usize| u8::from_str_radix(hex.get(start..start + 2)?, 16).ok();
    match hex.len() {
        6 => Some(Rgba {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
            a: 255,
        }),
        8 => Some(Rgba {
            r: channel(0)?,
            g: channel(2)?,
            b: channel(4)?,
            a: channel(6)?,
        }),
        _ => None,
    }
}

impl Rgba {
    /// Composite this colour over an opaque backdrop, in sRGB space.
    #[must_use]
    pub fn over(self, backdrop: Self) -> Self {
        let alpha = f32::from(self.a) / 255.0;
        let blend = |src: u8, dst: u8| -> u8 {
            let value = f32::from(src) * alpha + f32::from(dst) * (1.0 - alpha);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            {
                value.round().clamp(0.0, 255.0) as u8
            }
        };
        let out_alpha = f32::from(self.a) + f32::from(backdrop.a) * (1.0 - alpha);
        Self {
            r: blend(self.r, backdrop.r),
            g: blend(self.g, backdrop.g),
            b: blend(self.b, backdrop.b),
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            a: out_alpha.round().clamp(0.0, 255.0) as u8,
        }
    }
}

/// WCAG relative luminance of an sRGB colour. Alpha is ignored; composite
/// translucent colours over their background first.
#[must_use]
pub fn relative_luminance(color: Rgba) -> f64 {
    fn linear(channel: u8) -> f64 {
        let s = f64::from(channel) / 255.0;
        if s <= 0.040_45 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
}

/// WCAG contrast ratio in `1.0..=21.0`.
#[must_use]
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let first = relative_luminance(a);
    let second = relative_luminance(b);
    let (lighter, darker) = if first >= second {
        (first, second)
    } else {
        (second, first)
    };
    (lighter + 0.05) / (darker + 0.05)
}

// ------------------------------------------------------------------ helpers

struct ResolvedRun<'a> {
    text: &'a str,
    size: f32,
    color: Rgba,
    bold: bool,
}

struct ContrastSpec<'a> {
    subject: &'a str,
    foreground: Rgba,
    background: Rgba,
    size: f32,
    bold: bool,
}

fn resolved_runs<'a>(object: &'a Object, theme: &Theme) -> Vec<ResolvedRun<'a>> {
    let (text, style, runs) = match &object.kind {
        ObjectKind::Text {
            text, style, runs, ..
        } => (text.as_str(), style, runs.as_slice()),
        ObjectKind::Shape {
            text: Some(text), ..
        } => (text.text.as_str(), &text.style, text.runs.as_slice()),
        _ => return Vec::new(),
    };
    let style_color = parse_color(&style.color)
        .or_else(|| parse_color(&theme.text))
        .unwrap_or(DEFAULT_TEXT);
    let base_size = if style.size.is_finite() && style.size > 0.0 {
        style.size
    } else {
        20.0
    };
    if runs.is_empty() {
        return vec![ResolvedRun {
            text,
            size: base_size,
            color: style_color,
            bold: style.bold,
        }];
    }
    runs.iter()
        .take(1000)
        .map(|run| {
            let size = run.size.unwrap_or(base_size);
            let size = if size.is_finite() && size > 0.0 {
                size
            } else {
                20.0
            };
            let color = run
                .color
                .as_ref()
                .and_then(parse_color)
                .unwrap_or(style_color);
            ResolvedRun {
                text: run.text.as_str(),
                size,
                color,
                bold: run.bold.unwrap_or(style.bold),
            }
        })
        .collect()
}

fn resolve_theme_text(theme: &Theme) -> Rgba {
    parse_color(&theme.text).unwrap_or(DEFAULT_TEXT)
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl Frame {
    fn from_rect(rect: &Rect) -> Option<Self> {
        if !rect.x.is_finite()
            || !rect.y.is_finite()
            || !rect.w.is_finite()
            || !rect.h.is_finite()
            || rect.w < 0.0
            || rect.h < 0.0
        {
            return None;
        }
        let right = rect.x + rect.w;
        let bottom = rect.y + rect.h;
        if !right.is_finite() || !bottom.is_finite() {
            return None;
        }
        Some(Self {
            x: rect.x,
            y: rect.y,
            w: rect.w,
            h: rect.h,
        })
    }

    fn contains(self, other: Self) -> bool {
        self.x <= other.x
            && self.y <= other.y
            && self.x + self.w >= other.x + other.w
            && self.y + self.h >= other.y + other.h
    }

    fn overlaps(self, other: Self) -> bool {
        self.x < other.x + other.w
            && other.x < self.x + self.w
            && self.y < other.y + other.h
            && other.y < self.y + self.h
    }
}

enum Background {
    Known(Rgba),
    Unknown,
}

fn measure_background(
    course: &Course,
    slide: &Slide,
    text: Frame,
    below: &[&Object],
) -> Background {
    let mut background = parse_color(course.slide_background(slide))
        .unwrap_or(WHITE)
        .over(WHITE);
    for object in below {
        let Some(candidate) = Frame::from_rect(&object.frame) else {
            continue;
        };
        if !candidate.overlaps(text) {
            continue;
        }
        match &object.kind {
            ObjectKind::Image { .. } | ObjectKind::Video { .. } => return Background::Unknown,
            ObjectKind::Shape { fill, .. } => {
                let Some(fill) = parse_color(fill) else {
                    continue;
                };
                if fill.a == 0 {
                    continue;
                }
                if !candidate.contains(text) {
                    return Background::Unknown;
                }
                background = fill.over(background);
            }
            _ => {}
        }
    }
    Background::Known(background)
}

fn is_interactive(slide: &Slide, object: &Object) -> bool {
    matches!(
        object.kind,
        ObjectKind::Button { .. } | ObjectKind::Marker { .. } | ObjectKind::ScrollPanel { .. }
    ) || has_pointer_trigger(slide, &object.id)
        || is_question_target(slide, &object.id)
}

fn has_pointer_trigger(slide: &Slide, id: &str) -> bool {
    all_triggers(slide).any(|trigger| match &trigger.when {
        Event::Click { object } | Event::HoverStart { object } | Event::HoverEnd { object } => {
            object.as_str() == id
        }
        _ => false,
    })
}

fn is_question_target(slide: &Slide, id: &str) -> bool {
    let Some(question) = &slide.question else {
        return false;
    };
    question.hotspot_image.as_deref() == Some(id)
        || question
            .drag_items
            .iter()
            .any(|drag| drag.item.as_str() == id)
        || question
            .drop_targets
            .iter()
            .any(|target| target.as_str() == id)
}

fn has_pause_or_stop(slide: &Slide, id: &str) -> bool {
    all_triggers(slide).any(|trigger| {
        matches!(
            &trigger.action,
            crate::Action::PauseMedia { object } | crate::Action::StopMedia { object }
                if object.as_str() == id
        )
    })
}

fn all_triggers(slide: &Slide) -> impl Iterator<Item = &crate::Trigger> {
    slide
        .triggers
        .iter()
        .chain(slide.layers.iter().flat_map(|layer| layer.triggers.iter()))
}

fn has_description(object: &Object) -> bool {
    object
        .description
        .as_deref()
        .is_some_and(|d| !d.trim().is_empty())
}

fn has_label(object: &Object) -> bool {
    match &object.kind {
        ObjectKind::Button { label, .. } => !label.trim().is_empty(),
        ObjectKind::Marker { title, body, .. } => {
            !title.trim().is_empty() || !body.trim().is_empty()
        }
        ObjectKind::Text { text, runs, .. } => {
            !text.trim().is_empty() || runs.iter().any(|run| !run.text.trim().is_empty())
        }
        ObjectKind::Image { alt, .. } => !alt.trim().is_empty(),
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            !text.text.trim().is_empty() || text.runs.iter().any(|run| !run.text.trim().is_empty())
        }
        _ => false,
    }
}

fn snippet(text: &str) -> String {
    let text = text.trim();
    let mut short: String = text.chars().take(40).collect();
    if text.chars().count() > 40 {
        short.push_str("...");
    }
    short
}

#[cfg(test)]
mod tests;
