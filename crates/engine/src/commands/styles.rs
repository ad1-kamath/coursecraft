//! Palette, course-wide recolour and named text styles. Each command is one
//! undo step. Objects keep concrete colours and styles; these commands write
//! them course-wide, and the renderer and player never read a style by name.

use coursecraft_course::{
    Color, Course, MAX_PALETTE_COLORS, MAX_TEXT_STYLES, NamedTextStyle, Object, ObjectKind,
    PaletteColor, Paragraph, Slide, TextStyle, apply_named_style, normalize_hex, recolor_course,
    style_of_text_box,
};
use serde::Deserialize;
use serde_json::{Value as Json, json};

use super::{bad, find_object_mut, fresh_id, keep_valid, objects_param, opt, req, slide_mut, text};
use crate::{EngineError, Result, Session};

const MAX_NAME: usize = 64;

/// `#RRGGBB` or `#RRGGBBAA`, upper case, from text the caller gave.
fn color_param(cmd: &str, what: &str, raw: &str) -> Result<Color> {
    normalize_hex(raw).map(Color).ok_or_else(|| {
        bad(
            cmd,
            format!("{what} {raw:?} is not a colour such as #2F6FEB"),
        )
    })
}

fn name_param(cmd: &str, raw: String, what: &str) -> Result<String> {
    let name: String = raw.chars().filter(|c| !c.is_control()).collect();
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(bad(cmd, format!("{what} name is empty")));
    }
    if name.chars().count() > MAX_NAME {
        return Err(bad(
            cmd,
            format!("{what} name is longer than {MAX_NAME} characters"),
        ));
    }
    text(cmd, name, "name")
}

fn id_param(cmd: &str, id: &str) -> Result<()> {
    super::validate_id(cmd, id)?;
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(bad(cmd, format!("id {id:?} must use only [A-Za-z0-9_-]")));
    }
    Ok(())
}

// ------------------------------------------------------------------ palette

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PaletteInput {
    #[serde(default)]
    id: Option<String>,
    name: String,
    color: String,
}

/// `theme.set_palette {colors: [{id?, name, color}]}`: replace the palette.
pub(super) fn theme_set_palette(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "theme.set_palette";
    let colors: Vec<PaletteInput> = req(C, p, "colors")?;
    if colors.len() > MAX_PALETTE_COLORS {
        return Err(bad(
            C,
            format!("a palette holds at most {MAX_PALETTE_COLORS} colours"),
        ));
    }
    let mut taken: Vec<String> = Vec::new();
    for input in &colors {
        if let Some(id) = &input.id {
            id_param(C, id)?;
            if taken.contains(id) {
                return Err(bad(C, format!("duplicate palette id {id:?}")));
            }
            taken.push(id.clone());
        }
    }
    let mut palette = Vec::with_capacity(colors.len());
    for input in colors {
        let id = match input.id {
            Some(id) => id,
            None => {
                let id = fresh_id("pal", taken.iter().map(String::as_str));
                taken.push(id.clone());
                id
            }
        };
        palette.push(PaletteColor {
            id,
            name: name_param(C, input.name, "palette colour")?,
            color: color_param(C, "colour", &input.color)?,
        });
    }
    let ids: Vec<String> = palette.iter().map(|c| c.id.clone()).collect();
    s.course.theme.palette = palette;
    Ok(json!({ "ids": ids }))
}

/// `theme.set_palette_color {id, color, recolor?}`: change one palette colour
/// and, unless `recolor` is false, every use of its old colour in the course.
pub(super) fn theme_set_palette_color(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "theme.set_palette_color";
    let was_valid = s.course.is_playable();
    let id: String = req(C, p, "id")?;
    let raw: String = req(C, p, "color")?;
    let recolor: bool = opt(C, p, "recolor")?.unwrap_or(true);
    let new = color_param(C, "colour", &raw)?;
    let index = s
        .course
        .theme
        .palette
        .iter()
        .position(|c| c.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("palette colour {id:?} does not exist")))?;
    let old = s
        .course
        .theme
        .palette
        .get(index)
        .map(|c| c.color.0.clone())
        .unwrap_or_default();
    let mut changed = 0;
    if recolor {
        // The other palette entries keep their own colours, even when equal.
        let (course, count) =
            recolor_course(&s.course, &old, &new.0, false).map_err(|e| bad(C, e.to_string()))?;
        s.course = course;
        changed = count;
    }
    if let Some(entry) = s.course.theme.palette.get_mut(index) {
        entry.color = new;
    }
    keep_valid(s, was_valid)?;
    Ok(json!({ "changed": changed }))
}

/// `course.recolor {from, to}`: replace every use of a colour, course-wide.
pub(super) fn course_recolor(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "course.recolor";
    let was_valid = s.course.is_playable();
    let from: String = req(C, p, "from")?;
    let to: String = req(C, p, "to")?;
    color_param(C, "`from`", &from)?;
    color_param(C, "`to`", &to)?;
    let (course, changed) =
        recolor_course(&s.course, &from, &to, true).map_err(|e| bad(C, e.to_string()))?;
    s.course = course;
    keep_valid(s, was_valid)?;
    if changed == 0 {
        s.skip_undo = true;
    }
    Ok(json!({ "changed": changed }))
}

// -------------------------------------------------------------- text styles

fn style_index(c: &Course, id: &str) -> Result<usize> {
    c.text_styles
        .iter()
        .position(|n| n.id == id)
        .ok_or_else(|| EngineError::NotFound(format!("text style {id:?} does not exist")))
}

/// Every slide of the course: scenes, masters, templates and bank slides.
fn all_slides_mut(course: &mut Course) -> impl Iterator<Item = &mut Slide> {
    course
        .scenes
        .iter_mut()
        .flat_map(|scene| scene.slides.iter_mut())
        .chain(course.masters.iter_mut())
        .chain(course.templates.iter_mut())
        .chain(
            course
                .banks
                .iter_mut()
                .flat_map(|bank| bank.slides.iter_mut()),
        )
}

fn slide_objects_mut(slide: &mut Slide) -> impl Iterator<Item = &mut Object> {
    slide
        .objects
        .iter_mut()
        .chain(slide.layers.iter_mut().flat_map(|l| l.objects.iter_mut()))
}

/// Merge `patch` (an object of `TextStyle` fields) over `base`.
fn patched_style(cmd: &str, base: &TextStyle, patch: &Json) -> Result<TextStyle> {
    let Json::Object(fields) = patch else {
        return Err(bad(cmd, "`style` must be an object of text style fields"));
    };
    let mut merged = serde_json::to_value(base).map_err(|e| bad(cmd, e.to_string()))?;
    if let Json::Object(map) = &mut merged {
        for (key, value) in fields {
            map.insert(key.clone(), value.clone());
        }
    }
    let style: TextStyle =
        serde_json::from_value(merged).map_err(|e| bad(cmd, format!("`style`: {e}")))?;
    if !style.color.is_valid() {
        return Err(bad(
            cmd,
            format!("colour {:?} is not #RRGGBB or #RRGGBBAA", style.color.0),
        ));
    }
    Ok(style)
}

fn paragraph_param(cmd: &str, value: &Json) -> Result<Option<Paragraph>> {
    if value.is_null() {
        return Ok(None);
    }
    let paragraph: Paragraph = serde_json::from_value(value.clone())
        .map_err(|e| bad(cmd, format!("`paragraphs`: {e}")))?;
    Ok(Some(paragraph))
}

/// `text_style.create {name, from_object?: {slide, object}, style?}`.
///
/// With `from_object` the style is what that text box looks like now, and the
/// box is linked to it. With `style` (an object of text style fields over the
/// defaults) nothing is linked. Either way `paragraphs?` may add paragraph
/// formatting.
pub(super) fn text_style_create(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text_style.create";
    let name = name_param(C, req(C, p, "name")?, "text style")?;
    if s.course.text_styles.len() >= MAX_TEXT_STYLES {
        return Err(bad(
            C,
            format!("a course holds at most {MAX_TEXT_STYLES} text styles"),
        ));
    }
    let from: Option<FromObject> = opt(C, p, "from_object")?;
    let given: Option<Json> = opt(C, p, "style")?;
    if from.is_some() == given.is_some() {
        return Err(bad(C, "give exactly one of `from_object` and `style`"));
    }
    let id = fresh_id("style", s.course.text_styles.iter().map(|n| n.id.as_str()));
    let (style, mut paragraphs) = match (&from, &given) {
        (Some(from), _) => {
            let slide = slide_mut(s, &from.slide)?;
            let object = find_object_mut(slide, &from.object)?;
            let Some(found) = style_of_text_box(&object.kind) else {
                return Err(bad(
                    C,
                    format!("object {:?} is not a text box", from.object),
                ));
            };
            if let ObjectKind::Text { text_style, .. } = &mut object.kind {
                *text_style = Some(id.clone());
            }
            found
        }
        (None, Some(style)) => (patched_style(C, &TextStyle::default(), style)?, None),
        (None, None) => return Err(bad(C, "give `from_object` or `style`")),
    };
    if let Some(value) = p.get("paragraphs") {
        paragraphs = paragraph_param(C, value)?;
    }
    s.course.text_styles.push(NamedTextStyle {
        id: id.clone(),
        name,
        style,
        paragraphs,
    });
    Ok(json!({ "id": id }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FromObject {
    slide: String,
    object: String,
}

/// Put the named style on every text box linked to it, in the whole course.
fn reapply(course: &mut Course, index: usize) -> usize {
    let Some(named) = course.text_styles.get(index).cloned() else {
        return 0;
    };
    let mut count = 0;
    for slide in all_slides_mut(course) {
        for object in slide_objects_mut(slide) {
            let linked = matches!(
                &object.kind,
                ObjectKind::Text { text_style: Some(id), .. } if *id == named.id
            );
            if linked && apply_named_style(&mut object.kind, &named, false) {
                count += 1;
            }
        }
    }
    count
}

/// `text_style.update {id, style?, name?, paragraphs?}`: change the style, then
/// put it on every linked box, including ones changed by hand. Per-range
/// overrides inside the text are kept.
pub(super) fn text_style_update(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text_style.update";
    let id: String = req(C, p, "id")?;
    let index = style_index(&s.course, &id)?;
    let name: Option<String> = opt(C, p, "name")?;
    let style: Option<Json> = opt(C, p, "style")?;
    let current = s
        .course
        .text_styles
        .get(index)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("text style {id:?} does not exist")))?;
    let new_style = match &style {
        Some(patch) => Some(patched_style(C, &current.style, patch)?),
        None => None,
    };
    let new_paragraphs = match p.get("paragraphs") {
        Some(value) => Some(paragraph_param(C, value)?),
        None => None,
    };
    let new_name = match name {
        Some(n) => Some(name_param(C, n, "text style")?),
        None => None,
    };
    if let Some(entry) = s.course.text_styles.get_mut(index) {
        if let Some(n) = new_name {
            entry.name = n;
        }
        if let Some(st) = new_style {
            entry.style = st;
        }
        if let Some(para) = new_paragraphs {
            entry.paragraphs = para;
        }
    }
    let updated = reapply(&mut s.course, index);
    Ok(json!({ "updated": updated }))
}

/// `text_style.delete {id}`: remove the style and unlink its boxes. Their look
/// stays as it is.
pub(super) fn text_style_delete(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text_style.delete";
    let id: String = req(C, p, "id")?;
    let index = style_index(&s.course, &id)?;
    s.course.text_styles.remove(index);
    let mut unlinked = 0;
    for slide in all_slides_mut(&mut s.course) {
        for object in slide_objects_mut(slide) {
            if let ObjectKind::Text { text_style, .. } = &mut object.kind
                && text_style.as_deref() == Some(id.as_str())
            {
                *text_style = None;
                unlinked += 1;
            }
        }
    }
    Ok(json!({ "unlinked": unlinked }))
}

/// `text_style.apply {id, slide, objects, clear_overrides?}`: give text boxes
/// the style and link them. Per-range overrides are kept unless
/// `clear_overrides`.
pub(super) fn text_style_apply(s: &mut Session, p: &Json) -> Result<Json> {
    const C: &str = "text_style.apply";
    let id: String = req(C, p, "id")?;
    let sid: String = req(C, p, "slide")?;
    let objects = objects_param(C, p)?;
    let clear: bool = opt(C, p, "clear_overrides")?.unwrap_or(false);
    let index = style_index(&s.course, &id)?;
    let named = s
        .course
        .text_styles
        .get(index)
        .cloned()
        .ok_or_else(|| EngineError::NotFound(format!("text style {id:?} does not exist")))?;
    let slide = slide_mut(s, &sid)?;
    for oid in &objects {
        let object = find_object_mut(slide, oid)?;
        if !apply_named_style(&mut object.kind, &named, clear) {
            return Err(bad(C, format!("object {oid:?} is not a text box")));
        }
    }
    Ok(json!({ "applied": objects.len() }))
}
