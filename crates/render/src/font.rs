//! Bundled font loading, custom course fonts, installed system fonts, and fallback resolution.

use std::collections::HashSet;
use std::sync::Arc;

use coursecraft_course::{Course, FontStyle};
use egui::{FontData, FontDefinitions, FontFamily, FontId};

use crate::media::{MediaFetch, MediaSource};

/// Loaded bytes and collection face index for an installed system font face.
#[derive(Clone, Debug)]
pub struct SystemFontData {
    pub bytes: Arc<[u8]>,
    pub face_index: u32,
}

/// Optional provider of installed system fonts (desktop only; absent on wasm).
pub trait SystemFontProvider: Send + Sync {
    /// Load the font face bytes for `(family, style)` if installed, falling
    /// back to `FontStyle::Regular` when the exact style is unavailable.
    fn load_face(&self, family: &str, style: FontStyle) -> Option<(FontStyle, SystemFontData)>;
}

#[cfg(not(target_arch = "wasm32"))]
fn to_media_style(style: FontStyle) -> coursecraft_media::FontFileStyle {
    match style {
        FontStyle::Regular => coursecraft_media::FontFileStyle::Regular,
        FontStyle::Bold => coursecraft_media::FontFileStyle::Bold,
        FontStyle::Italic => coursecraft_media::FontFileStyle::Italic,
        FontStyle::BoldItalic => coursecraft_media::FontFileStyle::BoldItalic,
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn from_media_style(style: coursecraft_media::FontFileStyle) -> FontStyle {
    match style {
        coursecraft_media::FontFileStyle::Regular => FontStyle::Regular,
        coursecraft_media::FontFileStyle::Bold => FontStyle::Bold,
        coursecraft_media::FontFileStyle::Italic => FontStyle::Italic,
        coursecraft_media::FontFileStyle::BoldItalic => FontStyle::BoldItalic,
    }
}

/// [`SystemFontProvider`] backed by [`coursecraft_media::system_fonts::SystemFontIndex`].
#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug, Clone)]
pub struct IndexFontProvider(pub Arc<coursecraft_media::system_fonts::SystemFontIndex>);

#[cfg(not(target_arch = "wasm32"))]
impl SystemFontProvider for IndexFontProvider {
    fn load_face(&self, family: &str, style: FontStyle) -> Option<(FontStyle, SystemFontData)> {
        let (_, face) = self.0.resolve_face(family, to_media_style(style))?;
        let bytes = self.0.load_face_bytes(face)?;
        Some((
            from_media_style(face.style),
            SystemFontData {
                bytes,
                face_index: face.face_index,
            },
        ))
    }
}

use coursecraft_media::text_metrics::{INTER_BOLD, INTER_BOLD_ITALIC, INTER_ITALIC, INTER_REGULAR};
const INTER_MEDIUM: &[u8] = include_bytes!("../fonts/Inter-Medium.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../fonts/Inter-SemiBold.ttf");
use coursecraft_media::text_metrics::SYMBOLS_FALLBACK;
const PHOSPHOR_REGULAR: &[u8] = include_bytes!("../fonts/Phosphor.ttf");
const PHOSPHOR_FILL: &[u8] = include_bytes!("../fonts/Phosphor-Fill.ttf");

/// `ctx.data` key holding `(face_count, fnv1a_hash)` of the course font set currently installed.
const COURSE_FONT_HASH_ID: &str = "coursecraft-course-font-hash";
/// `ctx.data` key holding the `Arc<HashSet<FontFamily>>` of currently installed families
/// for O(1) zero-clone lookup in [`family_exists`].
const INSTALLED_FAMILIES_ID: &str = "coursecraft-installed-font-families";
/// `ctx.data` key holding the monotonic `u64` epoch incremented on every font install.
const FONT_EPOCH_ID: &str = "coursecraft-font-epoch";
/// `ctx.data` key holding the optional `Arc<dyn SystemFontProvider>`.
const SYSTEM_PROVIDER_ID: &str = "coursecraft-system-font-provider";
/// `ctx.data` key holding `(family_lower, FontStyle)` pairs already looked up
/// via the system font provider, so missing or already-loaded faces are never
/// re-queried every frame.
const TRIED_SYSTEM_FACES_ID: &str = "coursecraft-tried-system-faces";
/// `ctx.data` key holding `(cumulative_pass_nr, Arc<FontDefinitions>)` queued via
/// `ctx.set_fonts` during the current pass, because egui only applies
/// `set_fonts` at the start of the next pass. Without tracking it here, a
/// second incremental font install in the same frame would clone stale
/// `ctx.fonts()` definitions and clobber the first font.
const PENDING_DEFS_ID: &str = "coursecraft-pending-font-defs";
/// `ctx.data` key holding `(cumulative_pass_nr, data_key, Arc::as_ptr)` for the
/// most recently queued `ctx.set_fonts` call, so downstream caches can tell
/// whether egui has applied the queued definitions to `ctx.fonts()` yet.
const PENDING_FONT_MARKER_ID: &str = "coursecraft-pending-font-marker";

const FNV_OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

fn fnv1a_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn fnv1a_str(hash: u64, s: &str) -> u64 {
    let hash = fnv1a_bytes(hash, &(s.len() as u64).to_le_bytes());
    fnv1a_bytes(hash, s.as_bytes())
}

pub(crate) fn base_definitions() -> FontDefinitions {
    base_definitions_from(FontDefinitions::default())
}

pub(crate) fn base_definitions_from(mut fonts: FontDefinitions) -> FontDefinitions {
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
        "symbols_fallback".to_owned(),
        Arc::new(FontData::from_static(SYMBOLS_FALLBACK)),
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
    // The symbol fallback sits before it: it covers marks such as the cross, check and star
    // that Inter lacks, and Phosphor only has private-use codepoints.
    default_proportional.push("symbols_fallback".to_owned());
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
    if fonts
        .families
        .get(&FontFamily::Monospace)
        .is_none_or(Vec::is_empty)
    {
        fonts
            .families
            .insert(FontFamily::Monospace, make_family("inter_regular"));
    }

    fonts
}

/// A character in course text that no bundled font can draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UndrawableChar {
    /// The slide id, when the text belongs to a slide.
    pub slide: Option<String>,
    /// The object id, when the text belongs to an object.
    pub object: Option<String>,
    /// The character that would draw as an empty box.
    pub ch: char,
}

/// List each character of the course text that no bundled font can draw, once per object.
///
/// A course font or an installed system font can still draw such a character on a machine
/// that has it, so a hit is a warning and not an error.
#[must_use]
pub fn undrawable_chars(course: &Course) -> Vec<UndrawableChar> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in coursecraft_course::course_strings(course) {
        for ch in item.text.chars() {
            if coursecraft_media::text_metrics::bundled_fonts_can_draw(ch) {
                continue;
            }
            let loc = &item.location;
            if seen.insert((loc.slide.clone(), loc.object.clone(), ch)) {
                out.push(UndrawableChar {
                    slide: loc.slide.clone(),
                    object: loc.object.clone(),
                    ch,
                });
            }
        }
    }
    out
}

/// Monotonic counter incremented each time font definitions are installed or updated on `ctx`.
///
/// Starts at `0` before any call to [`install_fonts`] or [`install_course_fonts`].
/// Downstream caches include this value in their keys so text layout is
/// invalidated whenever course or system fonts change.
#[must_use]
pub fn font_epoch(ctx: &egui::Context) -> u64 {
    ctx.data(|data| data.get_temp::<u64>(egui::Id::unique(FONT_EPOCH_ID)))
        .unwrap_or(0)
}

fn installed_course_font_key(ctx: &egui::Context) -> Option<(usize, u64)> {
    ctx.data(|data| data.get_temp(egui::Id::unique(COURSE_FONT_HASH_ID)))
}

fn store_course_font_key(ctx: &egui::Context, key: (usize, u64)) {
    ctx.data_mut(|data| data.insert_temp(egui::Id::unique(COURSE_FONT_HASH_ID), key));
}

fn pending_defs_for_pass(ctx: &egui::Context) -> Option<Arc<FontDefinitions>> {
    let pass = ctx.cumulative_pass_nr();
    let entry = ctx.data(|data| {
        data.get_temp::<(u64, Arc<FontDefinitions>)>(egui::Id::unique(PENDING_DEFS_ID))
    });
    match entry {
        Some((saved_pass, defs)) if saved_pass == pass => Some(defs),
        Some(_) => {
            ctx.data_mut(|data| {
                data.remove::<(u64, Arc<FontDefinitions>)>(egui::Id::unique(PENDING_DEFS_ID));
            });
            None
        }
        None => None,
    }
}

/// Return `true` when `ctx.set_fonts` was queued during the current pass and
/// egui has not yet applied those definitions to `ctx.fonts()`.
///
/// Steady frames with no pending font update take an O(1) `ctx.data` lookup
/// without touching `ctx.fonts()`.
pub(crate) fn fonts_pending_apply(ctx: &egui::Context) -> bool {
    let marker_id = egui::Id::unique(PENDING_FONT_MARKER_ID);
    let Some((saved_pass, key, ptr)) =
        ctx.data(|data| data.get_temp::<(u64, Arc<str>, usize)>(marker_id))
    else {
        return false;
    };
    if saved_pass != ctx.cumulative_pass_nr() {
        ctx.data_mut(|data| data.remove::<(u64, Arc<str>, usize)>(marker_id));
        return false;
    }
    let applied = ctx.fonts(|fonts| {
        fonts
            .definitions()
            .font_data
            .get(key.as_ref())
            .is_some_and(|data| Arc::as_ptr(data) as usize == ptr)
    });
    if applied {
        ctx.data_mut(|data| data.remove::<(u64, Arc<str>, usize)>(marker_id));
        false
    } else {
        true
    }
}

fn set_fonts_and_track(ctx: &egui::Context, defs: FontDefinitions, marker_key: &str) {
    let pass = ctx.cumulative_pass_nr();
    let families: Arc<HashSet<FontFamily>> = Arc::new(defs.families.keys().cloned().collect());
    let marker = defs.font_data.get(marker_key).map(|data| {
        (
            pass,
            Arc::<str>::from(marker_key),
            Arc::as_ptr(data) as usize,
        )
    });
    #[cfg(debug_assertions)]
    crate::media::test_counts::note_font_def_clone();
    let pending = Arc::new(defs.clone());
    ctx.data_mut(|data| {
        data.insert_temp(egui::Id::unique(PENDING_DEFS_ID), (pass, pending));
        data.insert_temp(egui::Id::unique(INSTALLED_FAMILIES_ID), families);
        let marker_id = egui::Id::unique(PENDING_FONT_MARKER_ID);
        if let Some(marker) = marker {
            data.insert_temp(marker_id, marker);
        } else {
            data.remove::<(u64, Arc<str>, usize)>(marker_id);
        }
        let epoch_id = egui::Id::unique(FONT_EPOCH_ID);
        let next_epoch = data.get_temp::<u64>(epoch_id).unwrap_or(0).wrapping_add(1);
        data.insert_temp(epoch_id, next_epoch);
    });
    ctx.set_fonts(defs);
}

fn install_base(ctx: &egui::Context) {
    if installed_course_font_key(ctx) == Some((0, 0)) {
        return;
    }
    set_fonts_and_track(ctx, base_definitions(), "inter_regular");
    clear_tried_system_faces(ctx);
    store_course_font_key(ctx, (0, 0));
}

/// Register (or clear) the optional system font provider on `ctx`.
///
/// Only desktop builds supply a provider; the web player leaves it unset.
pub fn set_system_font_provider(
    ctx: &egui::Context,
    provider: Option<Arc<dyn SystemFontProvider>>,
) {
    let id = egui::Id::unique(SYSTEM_PROVIDER_ID);
    let unchanged = ctx.data(|data| {
        let existing = data.get_temp::<Option<Arc<dyn SystemFontProvider>>>(id);
        match (
            existing.as_ref().and_then(|o| o.as_ref()),
            provider.as_ref(),
        ) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => existing.is_some(),
            _ => false,
        }
    });
    if unchanged {
        return;
    }
    ctx.data_mut(|data| {
        data.insert_temp(id, provider);
        let epoch_id = egui::Id::unique(FONT_EPOCH_ID);
        let next_epoch = data.get_temp::<u64>(epoch_id).unwrap_or(0).wrapping_add(1);
        data.insert_temp(epoch_id, next_epoch);
    });
    clear_tried_system_faces(ctx);
}

fn system_font_provider(ctx: &egui::Context) -> Option<Arc<dyn SystemFontProvider>> {
    ctx.data(|data| {
        data.get_temp::<Option<Arc<dyn SystemFontProvider>>>(egui::Id::unique(SYSTEM_PROVIDER_ID))
            .flatten()
    })
}

fn clear_tried_system_faces(ctx: &egui::Context) {
    ctx.data_mut(|data| {
        data.remove::<HashSet<(String, &'static str)>>(egui::Id::unique(TRIED_SYSTEM_FACES_ID));
    });
}

fn was_system_face_tried(ctx: &egui::Context, key: &(String, &'static str)) -> bool {
    ctx.data(|data| {
        data.get_temp::<HashSet<(String, &'static str)>>(egui::Id::unique(TRIED_SYSTEM_FACES_ID))
            .is_some_and(|set| set.contains(key))
    })
}

fn mark_system_face_tried(ctx: &egui::Context, key: (String, &'static str)) {
    ctx.data_mut(|data| {
        let id = egui::Id::unique(TRIED_SYSTEM_FACES_ID);
        let mut set = data
            .get_temp::<HashSet<(String, &'static str)>>(id)
            .unwrap_or_default();
        set.insert(key);
        data.insert_temp(id, set);
    });
}

/// Install the bundled Inter and Phosphor font families into an [`egui::Context`].
///
/// Calling this again never resets the font atlas, and it leaves any custom
/// course faces installed by [`install_course_fonts`] in place.
pub fn install_fonts(ctx: &egui::Context) {
    if installed_course_font_key(ctx).is_some() {
        return;
    }
    install_base(ctx);
}

/// A stable egui family name for one course face.
#[must_use]
pub fn system_face_family(family: &str, style: FontStyle) -> FontFamily {
    FontFamily::Name(
        format!(
            "coursecraft-sysfont:{}:{}",
            family.trim().to_ascii_lowercase(),
            style.as_str()
        )
        .into(),
    )
}
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

fn course_font_key(course: &Course, media: &dyn MediaSource) -> (usize, u64) {
    let mut hash = FNV_OFFSET_BASIS;
    for face in &course.fonts {
        let (status, len) = match media.get(&face.src) {
            MediaFetch::Ready(bytes) => (1u8, bytes.len() as u64),
            MediaFetch::Pending => (2u8, 0u64),
            MediaFetch::Missing => (3u8, 0u64),
        };
        let style_tag = match face.style {
            FontStyle::Regular => 0u8,
            FontStyle::Bold => 1u8,
            FontStyle::Italic => 2u8,
            FontStyle::BoldItalic => 3u8,
        };
        hash = fnv1a_str(hash, &face.family);
        hash = fnv1a_bytes(hash, &[style_tag, status]);
        hash = fnv1a_str(hash, &face.src);
        hash = fnv1a_bytes(hash, &len.to_le_bytes());
    }
    (course.fonts.len(), hash)
}

/// Register every course font face and one egui family per (family, style),
/// with Inter after it as the glyph fallback.
///
/// Safe to call every frame: the font atlas is only reset when the set of
/// faces, their sources or their loaded sizes change.
pub fn install_course_fonts(ctx: &egui::Context, course: &Course, media: &dyn MediaSource) {
    if course.fonts.is_empty() {
        if installed_course_font_key(ctx) != Some((0, 0)) {
            install_base(ctx);
        }
        return;
    }

    let key = course_font_key(course, media);
    if installed_course_font_key(ctx) == Some(key) {
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
        let Ok(faces) = coursecraft_media::font_collection_info(&bytes) else {
            continue;
        };
        let face_index = faces
            .iter()
            .find(|(_, info, aliases)| {
                (info.family.eq_ignore_ascii_case(&face.family)
                    || aliases.iter().any(|a| a.eq_ignore_ascii_case(&face.family)))
                    && to_course_style(info.style) == face.style
            })
            .or_else(|| {
                faces.iter().find(|(_, info, aliases)| {
                    info.family.eq_ignore_ascii_case(&face.family)
                        || aliases.iter().any(|a| a.eq_ignore_ascii_case(&face.family))
                })
            })
            .map_or(0, |(idx, _, _)| *idx);
        let data_key = format!("course-font-{index}");
        let mut font_data = FontData::from_owned(bytes.to_vec());
        font_data.index = face_index;
        fonts
            .font_data
            .insert(data_key.clone(), Arc::new(font_data));
        let mut list = Vec::with_capacity(1 + inter_fallback.len());
        list.push(data_key);
        // Inter follows the course face so a missing glyph never turns into a
        // box while the bundled font can draw it.
        list.extend(inter_fallback.iter().cloned());
        fonts
            .families
            .insert(course_face_family(&face.family, face.style), list);
    }

    set_fonts_and_track(ctx, fonts, "inter_regular");
    clear_tried_system_faces(ctx);
    store_course_font_key(ctx, key);
}

fn to_course_style(style: coursecraft_media::FontFileStyle) -> FontStyle {
    match style {
        coursecraft_media::FontFileStyle::Regular => FontStyle::Regular,
        coursecraft_media::FontFileStyle::Bold => FontStyle::Bold,
        coursecraft_media::FontFileStyle::Italic => FontStyle::Italic,
        coursecraft_media::FontFileStyle::BoldItalic => FontStyle::BoldItalic,
    }
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
    let installed = ctx.data(|data| {
        data.get_temp::<Arc<HashSet<FontFamily>>>(egui::Id::unique(INSTALLED_FAMILIES_ID))
    });
    if let Some(families) = installed {
        return families.contains(family);
    }
    ctx.fonts(|f| f.definitions().families.contains_key(family))
}

fn install_system_face_incrementally(
    ctx: &egui::Context,
    family_name: &str,
    style: FontStyle,
    data: SystemFontData,
) -> FontFamily {
    let target = system_face_family(family_name, style);
    if family_exists(ctx, &target) {
        return target;
    }
    let mut defs = match pending_defs_for_pass(ctx) {
        Some(pending) => {
            #[cfg(debug_assertions)]
            crate::media::test_counts::note_font_def_clone();
            (*pending).clone()
        }
        None => {
            #[cfg(debug_assertions)]
            crate::media::test_counts::note_font_def_clone();
            ctx.fonts(|f| f.definitions().clone())
        }
    };
    let data_key = format!(
        "sys-font:{}:{}",
        family_name.trim().to_ascii_lowercase(),
        style.as_str()
    );
    let mut font_data = FontData::from_owned(data.bytes.to_vec());
    font_data.index = data.face_index;
    defs.font_data.insert(data_key.clone(), Arc::new(font_data));
    let inter_fallback = defs
        .families
        .get(&FontFamily::Proportional)
        .cloned()
        .unwrap_or_else(|| vec!["inter_regular".to_owned()]);
    let mut list = Vec::with_capacity(1 + inter_fallback.len());
    list.push(data_key.clone());
    list.extend(inter_fallback);
    defs.families.insert(target.clone(), list);
    set_fonts_and_track(ctx, defs, &data_key);
    ctx.request_repaint();
    target
}

/// Choose a slide text font, resolving `family` to:
/// 1. course-embedded face (exact style, then regular)
/// 2. installed system face (exact style, then regular)
/// 3. Inter (exact style, then proportional).
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
        let requested_style = style_for(bold, italic);
        let exact = course_face_family(name, requested_style);
        if family_exists(ctx, &exact) {
            return FontId::new(size, exact);
        }
        let regular = course_face_family(name, FontStyle::Regular);
        if family_exists(ctx, &regular) {
            return FontId::new(size, regular);
        }

        // Installed system font (desktop only, when a provider is registered).
        let sys_exact = system_face_family(name, requested_style);
        if family_exists(ctx, &sys_exact) {
            return FontId::new(size, sys_exact);
        }
        let key = (name.to_ascii_lowercase(), requested_style.as_str());
        if !was_system_face_tried(ctx, &key) {
            mark_system_face_tried(ctx, key);
            if let Some(provider) = system_font_provider(ctx)
                && let Some((actual_style, data)) = provider.load_face(name, requested_style)
            {
                let installed = install_system_face_incrementally(ctx, name, actual_style, data);
                return FontId::new(size, installed);
            }
        }
        let sys_regular = system_face_family(name, FontStyle::Regular);
        if family_exists(ctx, &sys_regular) {
            return FontId::new(size, sys_regular);
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

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use coursecraft_course::FontFace;

    /// The symbol fallback is installed after Inter, so the cross, check and star draw.
    #[test]
    fn symbol_fallback_is_in_the_proportional_family() {
        let defs = base_definitions();
        let list = defs
            .families
            .get(&FontFamily::Proportional)
            .expect("proportional family");
        let inter = list
            .iter()
            .position(|n| n == "inter_regular")
            .expect("inter");
        let symbols = list
            .iter()
            .position(|n| n == "symbols_fallback")
            .expect("symbols");
        assert!(inter < symbols);
        for ch in [
            '\u{2715}', '\u{2713}', '\u{2714}', '\u{2717}', '\u{2605}', '\u{2606}',
        ] {
            assert!(coursecraft_media::text_metrics::bundled_fonts_can_draw(ch));
        }
    }

    #[test]
    fn undrawable_chars_names_the_object_and_the_character() {
        let course = Course::from_json(
            r#"{"schema_version":1,"id":"c","title":"T","slide_size":{"w":1280,"h":720},"scenes":[{"id":"sc0","title":"S","slides":[{"id":"s0","title":"Slide","objects":[
              {"id":"ok","kind":{"type":"button","label":"✕ Close ✓"},"frame":{"x":0,"y":0,"w":100,"h":40}},
              {"id":"bad","kind":{"type":"button","label":"नमस्ते"},"frame":{"x":0,"y":50,"w":100,"h":40}}
            ]}]}]}"#,
        )
        .expect("course parses");
        let found = undrawable_chars(&course);
        assert!(!found.is_empty());
        assert!(found.iter().all(|u| u.object.as_deref() == Some("bad")));
        assert!(found.iter().any(|u| u.ch == '\u{0928}'));
        assert!(!found.iter().any(|u| u.ch == '\u{2715}'));
    }

    struct StaticFontMedia;
    impl MediaSource for StaticFontMedia {
        fn get(&self, src: &str) -> MediaFetch {
            if src == "fonts/custom.ttf" {
                MediaFetch::Ready(Arc::from(INTER_BOLD))
            } else {
                MediaFetch::Missing
            }
        }
    }

    /// s2 / M34-05: `font_epoch` increments on `install_fonts` and `install_course_fonts`
    /// changes, stays stable on repeated unchanged calls, and `slide_font_named` performs
    /// zero `FontDefinitions` clones across 100 lookups.
    #[test]
    fn s2_font_epoch_bumps_on_install_and_zero_clones_on_lookup() {
        let ctx = egui::Context::default();
        assert_eq!(font_epoch(&ctx), 0, "fresh context starts at epoch 0");

        install_fonts(&ctx);
        let after_base = font_epoch(&ctx);
        assert_eq!(after_base, 1, "install_fonts should bump font_epoch to 1");

        // Repeated install_fonts is a no-op.
        install_fonts(&ctx);
        assert_eq!(font_epoch(&ctx), after_base);

        let empty_course = Course::from_json(
            r#"{"schema_version":1,"id":"empty","title":"Empty","slide_size":{"w":1280,"h":720},"scenes":[{"id":"sc0","title":"Scene 0","slides":[{"id":"s0","title":"Slide 0","objects":[]}]}]}"#,
        )
        .expect("empty course parses");

        // Empty-font course matches already-installed base fonts: no epoch bump.
        install_course_fonts(&ctx, &empty_course, &StaticFontMedia);
        assert_eq!(font_epoch(&ctx), after_base);

        // Adding a course font face bumps font_epoch.
        let mut custom_course = empty_course.clone();
        custom_course.fonts.push(FontFace {
            family: "CustomBrand".into(),
            style: FontStyle::Regular,
            src: "fonts/custom.ttf".into(),
        });
        install_course_fonts(&ctx, &custom_course, &StaticFontMedia);
        let after_custom = font_epoch(&ctx);
        assert_eq!(
            after_custom,
            after_base + 1,
            "adding a course font must bump font_epoch"
        );

        // Advance one pass so egui applies the queued fonts.
        ctx.run_ui(egui::RawInput::default(), |_ui| {})
            .drop_without_applying_deltas();

        #[cfg(debug_assertions)]
        crate::media::test_counts::reset_all();

        // Repeated install_course_fonts with unchanged fonts neither bumps epoch nor clones defs.
        for _ in 0..10 {
            install_course_fonts(&ctx, &custom_course, &StaticFontMedia);
        }
        assert_eq!(font_epoch(&ctx), after_custom);

        // 100 slide_font_named lookups perform 0 FontDefinitions clones.
        for _ in 0..100 {
            let custom = slide_font_named(&ctx, Some("CustomBrand"), false, false, 18.0);
            assert_eq!(
                custom.family,
                course_face_family("CustomBrand", FontStyle::Regular)
            );
            let inter = slide_font_named(&ctx, Some("Inter"), true, false, 18.0);
            assert_eq!(inter.family, FontFamily::Name("inter-bold".into()));
            let fallback = slide_font_named(&ctx, Some("UnknownFont"), false, false, 18.0);
            assert_eq!(fallback.family, FontFamily::Name("inter".into()));
        }

        #[cfg(debug_assertions)]
        assert_eq!(
            crate::media::test_counts::font_def_clone_count(),
            0,
            "unchanged install_course_fonts and 100 slide_font_named lookups must perform zero FontDefinitions clones"
        );

        // Removing course fonts resets to base and bumps epoch again.
        install_course_fonts(&ctx, &empty_course, &StaticFontMedia);
        assert_eq!(font_epoch(&ctx), after_custom + 1);
    }
}
