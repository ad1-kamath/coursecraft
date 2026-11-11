//! Named text styles: copy a style's look into a text box, and tell whether a
//! box still looks like the style it is linked to. Pure data, no behaviour the
//! renderer or player reads.

use crate::{Course, ListKind, NamedTextStyle, Object, ObjectKind, Paragraph, TextStyle};

/// How a text box relates to its named style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StyleLink {
    /// Not linked to a style, or not a text box.
    None,
    /// Linked to a style the course no longer has; the box keeps its own look.
    Missing,
    /// Linked, and the box looks exactly like the style.
    Matches,
    /// Linked, but the box was changed by hand ("modified").
    Modified,
}

/// Copy a named style into a text box and link it. The box's own paragraph
/// text is untouched. Per-range overrides in `runs` are kept unless
/// `clear_overrides`, which strips their formatting but keeps their links.
/// Returns false, and changes nothing, when `kind` is not a text box.
pub fn apply_named_style(
    kind: &mut ObjectKind,
    named: &NamedTextStyle,
    clear_overrides: bool,
) -> bool {
    let ObjectKind::Text {
        text,
        style,
        runs,
        paragraphs,
        text_style,
        ..
    } = kind
    else {
        return false;
    };
    *style = named.style.clone();
    *text_style = Some(named.id.clone());
    if let Some(wanted) = &named.paragraphs {
        let mut list = crate::text::paragraphs_for(text, paragraphs);
        for paragraph in &mut list {
            paragraph.line_spacing = wanted.line_spacing;
            paragraph.space_before = wanted.space_before;
            paragraph.space_after = wanted.space_after;
            if wanted.list != ListKind::None {
                paragraph.list = wanted.list;
            }
            if wanted.level != 0 {
                paragraph.level = wanted.level;
            }
        }
        while list.last().is_some_and(|p| *p == Paragraph::default()) {
            list.pop();
        }
        *paragraphs = list;
    }
    if clear_overrides {
        for run in runs.iter_mut() {
            run.bold = None;
            run.italic = None;
            run.underline = None;
            run.strikethrough = None;
            run.baseline = None;
            run.letter_spacing = None;
            run.size = None;
            run.color = None;
            run.font = None;
        }
        if runs.iter().all(|run| run.link.is_none()) {
            runs.clear();
        }
    }
    true
}

/// The base style and first-paragraph formatting of a text box, as a named
/// style would hold them. `None` when `kind` is not a text box. The paragraph
/// part is `None` when the box has no non-default paragraph formatting.
#[must_use]
pub fn style_of_text_box(kind: &ObjectKind) -> Option<(TextStyle, Option<Paragraph>)> {
    let ObjectKind::Text {
        style, paragraphs, ..
    } = kind
    else {
        return None;
    };
    let first = paragraphs.first().filter(|p| **p != Paragraph::default());
    Some((style.clone(), first.cloned()))
}

/// How `object` relates to the named style it is linked to.
#[must_use]
pub fn style_link(course: &Course, object: &Object) -> StyleLink {
    let ObjectKind::Text {
        text,
        style,
        paragraphs,
        text_style,
        ..
    } = &object.kind
    else {
        return StyleLink::None;
    };
    let Some(id) = text_style else {
        return StyleLink::None;
    };
    let Some(named) = course.text_styles.iter().find(|s| &s.id == id) else {
        return StyleLink::Missing;
    };
    if *style != named.style {
        return StyleLink::Modified;
    }
    let Some(wanted) = &named.paragraphs else {
        return StyleLink::Matches;
    };
    let has = crate::text::paragraphs_for(text, paragraphs);
    let same = has.iter().all(|p| {
        p.line_spacing == wanted.line_spacing
            && p.space_before == wanted.space_before
            && p.space_after == wanted.space_after
            && (wanted.list == ListKind::None || p.list == wanted.list)
            && (wanted.level == 0 || p.level == wanted.level)
    });
    if same {
        StyleLink::Matches
    } else {
        StyleLink::Modified
    }
}
