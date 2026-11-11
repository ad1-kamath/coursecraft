//! Bundled font loading, custom course fonts, and fallback resolution.

use std::sync::Arc;

use coursecraft_course::{Course, FontStyle};
use egui::{FontData, FontDefinitions, FontFamily, FontId};

use crate::media::{MediaFetch, MediaSource};

const INTER_REGULAR: &[u8] = include_bytes!("../fonts/Inter-Regular.ttf");
const INTER_MEDIUM: &[u8] = include_bytes!("../fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../fonts/Inter-SemiBold.ttf");
const INTER_BOLD: &[u8] = include_bytes!("../fonts/Inter-Bold.ttf");
const INTER_ITALIC: &[u8] = include_bytes!("../fonts/Inter-Italic.ttf");
const INTER_BOLD_ITALIC: &[u8] = include_bytes!("../fonts/Inter-BoldItalic.ttf");
const PHOSPHOR_REGULAR: &[u8] = include_bytes!("../fonts/Phosphor.ttf");
const PHOSPHOR_FILL: &[u8] = include_bytes!("../fonts/Phosphor-Fill.ttf");

/// `ctx.data` key holding the fingerprint of the font set currently installed.
const FINGERPRINT_ID: &str = "coursecraft-installed-fonts";

/// Which file is backing a registered family. `Ready(len)` means the bytes
/// are available; `Missing` and `Pending` both omit the face until it arrives.
#[derive(Clone, Debug, PartialEq, Eq)]
struct FaceFingerprint {
    family: String,
    style: FontStyle,
    src: String,
    status: u8,
    len: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct FontFingerprint {
    faces: Vec<FaceFingerprint>,
}

fn base_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::default();

    fonts.font_data.insert(
        "inter_regular".to_owned(),
        Arc::new(FontData::from_static(INTER_REGULAR)),
    );
    fonts.font_data.insert(
        "inter_medium".to_owned(),
        Arc::new(FontData::from_static(INTER_MEDIUM)),
    );
    fonts.font_data.insert(
        "inter_semibold".to_owned(),
        Arc::new(FontData::from_static(INTER_SEMIBOLD)),
    );
    fonts.font_data.insert(
        "inter_bold".to_owned(),
        Arc::new(FontData::from_static(INTER_BOLD)),
    );
    fonts.font_data.insert(
        "inter_italic".to_owned(),
        Arc::new(FontData::from_static(INTER_ITALIC)),
    );
    fonts.font_data.insert(
        "inter_bold_italic".to_owned(),
        Arc::new(FontData::from_static(INTER_BOLD_ITALIC)),
    );
    fonts.font_data.insert(
        "phosphor_regular".to_owned(),
        Arc::new(FontData::from_static(PHOSPHOR_REGULAR)),
    );
    fonts.font_data.insert(
        "phosphor_fill".to_owned(),
        Arc::new(FontData::from_static(PHOSPHOR_FILL)),
    );

    let mut default_proportional = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_default();
    // Phosphor icon font is the last fallback of Proportional, so an icon codepoint
    // inside a normal label string will resolve cleanly without manual layout splitting.
    default_proportional.push("phosphor_regular".to_owned());

    let make_family = |primary: &str| -> Vec<String> {
        let mut list = Vec::with_capacity(1 + default_proportional.len());
        list.push(primary.to_owned());
        list.extend(default_proportional.clone());
        list
    };

    fonts.families.insert(
        FontFamily::Name("inter".into()),
        make_family("inter_regular"),
    );
    fonts.families.insert(
        FontFamily::Name("inter-medium".into()),
        make_family("inter_medium"),
    );
    fonts.families.insert(
        FontFamily::Name("inter-semibold".into()),
        make_family("inter_semibold"),
    );
    fonts.families.insert(
        FontFamily::Name("inter-bold".into()),
        make_family("inter_bold"),
    );
    fonts.families.insert(
        FontFamily::Name("inter-italic".into()),
        make_family("inter_italic"),
    );
    fonts.families.insert(
        FontFamily::Name("inter-bold-italic".into()),
        make_family("inter_bold_italic"),
    );
    fonts.families.insert(
        FontFamily::Name("phosphor".into()),
        vec!["phosphor_regular".to_owned()],
    );
    fonts.families.insert(
        FontFamily::Name("phosphor-fill".into()),
        vec!["phosphor_fill".to_owned()],
    );
    // Inter is also the default proportional face, so text set with plain
    // `FontId::proportional` matches the rest of the UI.
    fonts
        .families
        .insert(FontFamily::Proportional, make_family("inter_regular"));

    fonts
}

fn installed_fingerprint(ctx: &egui::Context) -> Option<FontFingerprint> {
    ctx.data(|data| data.get_temp(egui::Id::unique(FINGERPRINT_ID)))
}

fn store_fingerprint(ctx: &egui::Context, fingerprint: FontFingerprint) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(FINGERPRINT_ID), fingerprint));
}

fn install_base(ctx: &egui::Context) {
    let fingerprint = FontFingerprint::default();
    if installed_fingerprint(ctx).as_ref() == Some(&fingerprint) {
        return;
    }
    ctx.set_fonts(base_definitions());
    store_fingerprint(ctx, fingerprint);
}

/// Install the bundled Inter and Phosphor font families into an [`egui::Context`].
///
/// Calling this again never resets the font atlas, and it leaves any custom
/// course faces installed by [`install_course_fonts`] in place.
pub fn install_fonts(ctx: &egui::Context) {
    if installed_fingerprint(ctx).is_some() {
        return;
    }
    install_base(ctx);
}

/// A stable egui family name for one course face.
#[must_use]
pub fn course_face_family(family: &str, style: FontStyle) -> FontFamily {
    FontFamily::Name(
        format!(
            "coursecraft-font:{}:{}",
            family.trim().to_ascii_lowercase(),
            style.as_str()
        )
        .into(),
    )
}

fn fingerprint_for(course: &Course, media: &dyn MediaSource) -> FontFingerprint {
    let mut faces = Vec::with_capacity(course.fonts.len());
    for face in &course.fonts {
        let (status, len) = match media.get(&face.src) {
            MediaFetch::Ready(bytes) => (1u8, bytes.len()),
            MediaFetch::Pending => (2, 0),
            MediaFetch::Missing => (3, 0),
        };
        faces.push(FaceFingerprint {
            family: face.family.clone(),
            style: face.style,
            src: face.src.clone(),
            status,
            len,
        });
    }
    FontFingerprint { faces }
}

/// Register every course font face and one egui family per (family, style),
/// with Inter after it as the glyph fallback.
///
/// Safe to call every frame: the font atlas is only reset when the set of
/// faces, their sources or their loaded sizes change.
pub fn install_course_fonts(ctx: &egui::Context, course: &Course, media: &dyn MediaSource) {
    if course.fonts.is_empty() {
        install_base(ctx);
        return;
    }

    let fingerprint = fingerprint_for(course, media);
    if installed_fingerprint(ctx).as_ref() == Some(&fingerprint) {
        return;
    }

    let mut fonts = base_definitions();
    let inter_fallback = fonts
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_else(|| vec!["inter_regular".to_owned()]);

    for (index, face) in course.fonts.iter().enumerate() {
        let MediaFetch::Ready(bytes) = media.get(&face.src) else {
            continue;
        };
        // Treat a hostile or corrupt file as missing; never feed egui bytes
        // that do not parse as a font.
        if coursecraft_media::font_info(&bytes).is_err() {
            continue;
        }
        let data_key = format!("course-font-{index}");
        fonts.font_data.insert(
            data_key.clone(),
            Arc::new(FontData::from_owned(bytes.to_vec())),
        );
        let mut list = Vec::with_capacity(1 + inter_fallback.len());
        list.push(data_key);
        // Inter follows the course face so a missing glyph never turns into a
        // box while the bundled font can draw it.
        list.extend(inter_fallback.iter().cloned());
        fonts
            .families
            .insert(course_face_family(&face.family, face.style), list);
    }

    ctx.set_fonts(fonts);
    store_fingerprint(ctx, fingerprint);
}

fn inter_family(bold: bool, italic: bool) -> &'static str {
    match (bold, italic) {
        (false, false) => "inter",
        (true, false) => "inter-bold",
        (false, true) => "inter-italic",
        (true, true) => "inter-bold-italic",
    }
}

fn style_for(bold: bool, italic: bool) -> FontStyle {
    match (bold, italic) {
        (false, false) => FontStyle::Regular,
        (true, false) => FontStyle::Bold,
        (false, true) => FontStyle::Italic,
        (true, true) => FontStyle::BoldItalic,
    }
}

fn family_exists(ctx: &egui::Context, family: &FontFamily) -> bool {
    ctx.fonts(|f| f.definitions().families.contains_key(family))
}

/// Choose a slide text font, resolving `family` to its exact face, then its
/// regular face, then Inter. An unknown or missing family falls back to Inter.
#[must_use]
pub fn slide_font_named(
    ctx: &egui::Context,
    family: Option<&str>,
    bold: bool,
    italic: bool,
    size: f32,
) -> FontId {
    let name = family.map(str::trim).filter(|name| !name.is_empty());

    if let Some(name) = name
        && !name.eq_ignore_ascii_case("Inter")
        && !name.eq_ignore_ascii_case("Open Sans")
    {
        let exact = course_face_family(name, style_for(bold, italic));
        if family_exists(ctx, &exact) {
            return FontId::new(size, exact);
        }
        let regular = course_face_family(name, FontStyle::Regular);
        if family_exists(ctx, &regular) {
            return FontId::new(size, regular);
        }
    }

    let target = FontFamily::Name(inter_family(bold, italic).into());
    let font_family = if family_exists(ctx, &target) {
        target
    } else {
        FontFamily::Proportional
    };
    FontId::new(size, font_family)
}

/// Choose the appropriate slide text font, falling back to [`FontFamily::Proportional`]
/// if [`install_fonts`] has not been called on `ctx`.
#[must_use]
pub fn slide_font(ctx: &egui::Context, bold: bool, italic: bool, size: f32) -> FontId {
    slide_font_named(ctx, None, bold, italic, size)
}
