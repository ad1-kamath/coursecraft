//! A screen-reader description of the current player frame.
//!
//! egui paints to a canvas, which assistive technology cannot read. This module
//! turns the same composition the canvas draws from into a neutral tree of
//! headings, text, images and controls. It makes no play-time decisions and is
//! pure so the reading order and announcements can be tested without a browser.

use coursecraft_course::{Link, Object, ObjectKind, QuestionKind, Slide, Value, fill};
use coursecraft_logic::Player;
use egui::Rect;

use crate::compose::{DrawItem, Hit, ReviewMark, Visual, validate_frame};
use crate::focus::{ChromeButton, FocusTarget, build_list_with_notice};
use crate::player_frame::{PlayerFrame, review_banner_text};

/// What the live region says when the learner submits an unanswered question.
///
/// `answer_required` is a runtime effect rather than player state, so the web
/// layer announces it directly instead of deriving it from a tree change.
pub const ANSWER_REQUIRED_ANNOUNCEMENT: &str = "Please answer the question";

/// What a screen reader should call a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessRole {
    Heading,
    Text,
    Image,
    Button,
    Radio,
    Checkbox,
    TextBox,
    /// A multiline text-entry field.
    TextArea,
    Select,
    /// A group of radio buttons.
    RadioGroup,
    /// The hotspot question's marker control, announced as a slider.
    Slider,
    /// Text that opens a URL or jumps to another slide.
    Link,
    /// A table container. Rows and cells follow it in reading order.
    Table,
    /// One table row.
    Row,
    /// One table cell.
    Cell,
    /// A table cell in the header row, announced as a column header.
    ColumnHeader,
    /// The lightbox container, announced as a modal dialog.
    Dialog,
    /// A countdown timer, announced with its current value.
    Timer,
    /// A progress meter, announced as a progress bar with min/max/now.
    ProgressBar,
    /// A tab button in the drawer; `pressed` marks the selected tab.
    Tab,
    /// One row of a list, such as a glossary term.
    ListItem,
}

/// One item in the accessible reading order.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessNode {
    /// Stable across rebuilds, so a DOM layer can reconcile instead of replacing.
    pub key: String,
    pub role: AccessRole,
    pub label: String,
    /// Fill-in text or the selected matching value.
    pub value: Option<String>,
    /// Select options as `(value, label)`; an empty value is "Choose…".
    pub options: Vec<(String, String)>,
    pub checked: Option<bool>,
    /// A held drag item.
    pub pressed: Option<bool>,
    /// `aria-expanded`, for a marker's label.
    pub expanded: Option<bool>,
    /// `aria-description` text.
    pub hint: Option<String>,
    /// The keyboard target, or `None` for non-interactive content.
    pub target: Option<FocusTarget>,
    /// Slide coordinates, or screen coordinates when [`AccessNode::screen`].
    pub rect: Rect,
    pub screen: bool,
}

/// Everything a screen reader needs to describe the current frame.
#[derive(Debug, Clone, PartialEq)]
pub struct AccessTree {
    pub title: String,
    pub nodes: Vec<AccessNode>,
}

/// Build the reading order for the current player frame.
///
/// Interactive nodes appear in exactly the order of
/// [`crate::focus::build_list`], which is what the keyboard focus manager uses.
#[must_use]
pub fn build_access_tree(
    player: &Player<'_>,
    frame: &PlayerFrame,
    items: &[DrawItem],
    show_nav: bool,
    show_menu: bool,
) -> AccessTree {
    let mut nodes = Vec::new();
    let Some(slide) = player.current_slide() else {
        return AccessTree {
            title: String::new(),
            nodes,
        };
    };
    let list = if frame.ended {
        Vec::new()
    } else {
        build_list_with_notice(
            player,
            show_nav,
            show_menu,
            frame.drawer_open(),
            frame.showing_notice().is_some(),
            player.is_reviewing(),
            frame.focus().chrome_visibility(),
            frame.focus().drawer_view(),
        )
    };
    let title = player.display_text(&slide.title);
    let base_hidden = player.base_objects_hidden();

    // The notice banner's text, when it is showing, is read before everything,
    // with its dismiss button first in the focus order right beside it.
    notice_nodes(player, frame, items, slide, &list, &mut nodes);

    // A lightbox is a modal dialog named by its slide's title. Everything that
    // follows belongs to it in the reading order.
    if player.lightbox_open() {
        nodes.push(AccessNode {
            key: format!("dialog:{}", slide.id),
            role: AccessRole::Dialog,
            label: if title.trim().is_empty() {
                player.course().chrome_string("lightbox").to_owned()
            } else {
                title.clone()
            },
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: None,
            rect: Rect::NOTHING,
            screen: false,
        });
    }

    nodes.push(AccessNode {
        key: format!("heading:{}", slide.id),
        role: AccessRole::Heading,
        label: title.clone(),
        value: None,
        options: Vec::new(),
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: None,
        rect: Rect::NOTHING,
        screen: false,
    });

    // Master text and image alternatives are read before the slide's own.
    // Master objects are never in the keyboard focus list.
    push_master_nodes(player, slide, &mut nodes);

    // 2. Objects with an author focus order, in the order the manager uses.
    for target in &list {
        let FocusTarget::Object(id) = target else {
            continue;
        };
        let Some((object, _)) = slide.object(id) else {
            continue;
        };
        if object.focus_order.is_none() {
            continue;
        }
        if let Some(node) = target_node(player, frame, items, slide, target) {
            nodes.push(node);
        }
        push_marker_card(player, slide, object, &mut nodes);
        push_link_nodes(
            player,
            slide,
            items,
            id,
            &format!("object:{}:{}", slide.id, id),
            &mut nodes,
        );
    }

    // 3-5. Base content, the question and interactive base objects, unless a
    // visible layer hides the base.
    push_base_nodes(player, frame, items, slide, &list, base_hidden, &mut nodes);

    // 6. Visible layers, bottom to top, each in composition order.
    for layer_id in &player.state().visible_layers {
        let Some(layer) = slide.layer(layer_id) else {
            continue;
        };
        for object in &layer.objects {
            if !player.object_in_panel_viewport(&object.id) {
                continue;
            }
            let target = FocusTarget::Object(object.id.clone());
            if object.focus_order.is_some() && list.contains(&target) {
                continue;
            }
            if list.contains(&target) {
                if let Some(node) = target_node(player, frame, items, slide, &target) {
                    nodes.push(node);
                }
                push_marker_card(player, slide, object, &mut nodes);
                push_link_nodes(
                    player,
                    slide,
                    items,
                    &object.id,
                    &format!("layer:{}:{}:{}", slide.id, layer_id, object.id),
                    &mut nodes,
                );
            } else if content_object(&list, object) {
                let key = format!("layer:{}:{}:{}", slide.id, layer_id, object.id);
                if matches!(object.kind, ObjectKind::Table { .. }) {
                    push_table_nodes(player, slide, items, object, &key, &mut nodes);
                } else if let Some(node) = content_node(player, slide, items, object, key.clone()) {
                    nodes.push(node);
                }
                push_link_nodes(player, slide, items, &object.id, &key, &mut nodes);
            }
        }
    }

    // 7. Chrome, in focus order. The notice's dismiss button was read with the
    // notice text instead, first in the focus order.
    for target in &list {
        if target.is_chrome()
            && target != &FocusTarget::Chrome(ChromeButton::DismissNotice)
            && target != &FocusTarget::Chrome(ChromeButton::ExitReview)
            && let Some(node) = target_node(player, frame, items, slide, target)
        {
            nodes.push(node);
        }
    }

    AccessTree { title, nodes }
}

/// Read the slide's base content, question and interactive base objects.
///
/// Split from [`build_access_tree`] so the reading order stays a simple list of
/// steps. When `base_hidden` is set a visible layer has hidden the base, so none
/// of these nodes are read; master nodes are added separately and still read.
fn push_base_nodes(
    player: &Player<'_>,
    frame: &PlayerFrame,
    items: &[DrawItem],
    slide: &Slide,
    list: &[FocusTarget],
    base_hidden: bool,
    nodes: &mut Vec<AccessNode>,
) {
    if base_hidden {
        return;
    }

    // Non-interactive base objects, bottom to top.
    for object in &slide.objects {
        if !player.object_in_panel_viewport(&object.id) {
            continue;
        }
        if content_object(list, object) {
            let key = format!("object:{}:{}", slide.id, object.id);
            if matches!(object.kind, ObjectKind::Table { .. }) {
                push_table_nodes(player, slide, items, object, &key, nodes);
            } else if let Some(node) = content_node(player, slide, items, object, key.clone()) {
                nodes.push(node);
            }
            push_link_nodes(player, slide, items, &object.id, &key, nodes);
        }
    }

    // The question prompt, then its controls in focus order.
    if let Some(question) = slide.question.as_ref() {
        nodes.push(AccessNode {
            key: format!("prompt:{}", slide.id),
            role: AccessRole::Text,
            label: player.display_text(&question.prompt),
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: None,
            rect: question_prompt_rect(player),
            screen: false,
        });
        for target in list {
            if !is_question_control(target) {
                continue;
            }
            if let Some(node) = target_node(player, frame, items, slide, target) {
                nodes.push(node);
            }
        }
    }

    // Interactive base objects without an author focus order.
    for object in &slide.objects {
        if !player.object_in_panel_viewport(&object.id) {
            continue;
        }
        let target = FocusTarget::Object(object.id.clone());
        if object.focus_order.is_some() || !list.contains(&target) {
            continue;
        }
        if let Some(node) = target_node(player, frame, items, slide, &target) {
            nodes.push(node);
        }
        push_marker_card(player, slide, object, nodes);
        push_link_nodes(
            player,
            slide,
            items,
            &object.id,
            &format!("object:{}:{}", slide.id, object.id),
            nodes,
        );
    }
}

/// Read the notice banner first, when it is showing: its text, then its
/// dismiss button, first in the focus order beside it.
fn notice_nodes(
    player: &Player<'_>,
    frame: &PlayerFrame,
    items: &[DrawItem],
    slide: &Slide,
    list: &[FocusTarget],
    nodes: &mut Vec<AccessNode>,
) {
    if let Some(text) = frame.showing_notice() {
        nodes.push(AccessNode {
            key: "notice".to_owned(),
            role: AccessRole::Text,
            label: text.to_owned(),
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: None,
            rect: frame.notice_rect().unwrap_or(Rect::NOTHING),
            screen: true,
        });
        if let Some(target) = list.first()
            && target == &FocusTarget::Chrome(ChromeButton::DismissNotice)
            && let Some(node) = target_node(player, frame, items, slide, target)
        {
            nodes.push(node);
        }
    }
    if let Some(text) = review_banner_text(player) {
        nodes.push(AccessNode {
            key: "review".to_owned(),
            role: AccessRole::Text,
            label: text,
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: None,
            rect: frame.review_rect().unwrap_or(Rect::NOTHING),
            screen: true,
        });
        if let Some(target) = list
            .iter()
            .find(|target| target == &&FocusTarget::Chrome(ChromeButton::ExitReview))
            && let Some(node) = target_node(player, frame, items, slide, target)
        {
            nodes.push(node);
        }
    }
}

/// The live-region text for a tree change, if anything should be announced.
#[must_use]
pub fn announcement(
    previous: Option<&AccessTree>,
    current: &AccessTree,
    player: &Player<'_>,
) -> Option<String> {
    if let Some(text) = review_announcement(previous, current, player) {
        return Some(text);
    }
    if let Some(text) = notice_announcement(previous, current) {
        return Some(text);
    }
    if previous.is_none_or(|p| heading_key(p) != heading_key(current)) {
        return Some(slide_announcement(current, player));
    }
    let previous = previous?;
    if let Some(text) = newly_visible_layer(previous, current, player)
        && !text.is_empty()
    {
        return Some(text);
    }
    drag_announcement(previous, current, player).or_else(|| timer_announcement(previous, current))
}

/// Announce a timer's value only at each whole minute and in the last 10
/// seconds, not on every tick.
fn timer_announcement(previous: &AccessTree, current: &AccessTree) -> Option<String> {
    for node in &current.nodes {
        if node.role != AccessRole::Timer {
            continue;
        }
        let before = previous.nodes.iter().find(|n| n.key == node.key);
        let before_value = before.and_then(|n| n.value.as_deref());
        if before_value == node.value.as_deref() {
            continue;
        }
        let seconds: u64 = node
            .options
            .iter()
            .find(|(name, _)| name == "seconds")
            .and_then(|(_, text)| text.parse().ok())
            .unwrap_or(0);
        if seconds.is_multiple_of(60) || seconds <= 10 {
            let value = node.value.as_deref().unwrap_or_default();
            if value.is_empty() {
                continue;
            }
            return Some(format!("{}: {}", node.label, value));
        }
    }
    None
}

/// The review banner text when review starts, and again whenever the reviewed
/// slide changes.
fn review_announcement(
    previous: Option<&AccessTree>,
    current: &AccessTree,
    player: &Player<'_>,
) -> Option<String> {
    if !player.is_reviewing() {
        return None;
    }
    let text = review_banner_text(player)?;
    let was_reviewing =
        previous.is_some_and(|tree| tree.nodes.iter().any(|node| node.key == "review"));
    let slide_changed = previous.is_none_or(|tree| heading_key(tree) != heading_key(current));
    (!was_reviewing || slide_changed).then_some(text)
}

/// The notice text the first time it appears in the tree, so the learner hears
/// it once when the banner appears and not again on later frames.
fn notice_announcement(previous: Option<&AccessTree>, current: &AccessTree) -> Option<String> {
    let node = current.nodes.iter().find(|n| n.key == "notice")?;
    if previous.is_some_and(|previous| previous.nodes.iter().any(|n| n.key == "notice")) {
        return None;
    }
    (!node.label.is_empty()).then(|| node.label.clone())
}

fn heading_key(tree: &AccessTree) -> Option<&str> {
    tree.nodes
        .iter()
        .find(|n| n.role == AccessRole::Heading)
        .map(|n| n.key.as_str())
}

fn slide_announcement(current: &AccessTree, player: &Player<'_>) -> String {
    let index = player
        .current_slide()
        .and_then(|slide| player.slide_position(&slide.id))
        .map_or(1, |i| i.saturating_add(1));
    let total = player.play_order().len().max(1);
    fill(
        player.course().chrome_string("slideAnnouncement"),
        &[
            ("n", &index.to_string()),
            ("m", &total.to_string()),
            ("title", &current.title),
        ],
    )
}

fn newly_visible_layer(
    previous: &AccessTree,
    current: &AccessTree,
    player: &Player<'_>,
) -> Option<String> {
    let mut lines = Vec::new();
    let slide_id = player
        .current_slide()
        .map(|slide| slide.id.as_str())
        .unwrap_or_default();
    for layer_id in &player.state().visible_layers {
        let prefix = format!("layer:{slide_id}:{layer_id}:");
        let was_visible = previous.nodes.iter().any(|n| n.key.starts_with(&prefix));
        if was_visible {
            continue;
        }
        for node in &current.nodes {
            if node.key.starts_with(&prefix)
                && node.role == AccessRole::Text
                && !node.label.is_empty()
            {
                lines.push(node.label.clone());
            }
        }
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn drag_announcement(
    previous: &AccessTree,
    current: &AccessTree,
    player: &Player<'_>,
) -> Option<String> {
    let course = player.course();
    let template = course.chrome_string("dragOnTarget");
    for node in &current.nodes {
        if !node.key.starts_with("drag:") {
            continue;
        }
        let before = previous.nodes.iter().find(|n| n.key == node.key);
        let now_pressed = node.pressed == Some(true);
        let was_pressed = before.and_then(|n| n.pressed) == Some(true);
        let (base, target) = split_on_label(&node.label, template);
        if now_pressed && !was_pressed {
            return Some(fill(course.chrome_string("pickedUp"), &[("item", base)]));
        }
        if now_pressed && was_pressed {
            let previous_target = before
                .map(|n| split_on_label(&n.label, template).1)
                .unwrap_or_default();
            if let Some(target) = target
                && previous_target != Some(target)
            {
                return Some(fill(
                    course.chrome_string("itemOver"),
                    &[("item", base), ("target", target)],
                ));
            }
        }
        if !now_pressed
            && was_pressed
            && let Some(target) = target
        {
            return Some(fill(
                course.chrome_string("itemDropped"),
                &[("item", base), ("target", target)],
            ));
        }
    }
    None
}

/// Split a drag label built from the `{label}, on {target}` template back into
/// its parts. A translated template may move the placeholders but not reorder
/// them, so the literal text between them is the separator.
fn split_on_label<'a>(label: &'a str, template: &str) -> (&'a str, Option<&'a str>) {
    let Some(label_at) = template.find("{label}") else {
        return (label, None);
    };
    let Some(after_label) = template.get(label_at.saturating_add(7)..) else {
        return (label, None);
    };
    let Some(target_at) = after_label.find("{target}") else {
        return (label, None);
    };
    let prefix = template.get(..label_at).unwrap_or_default();
    let middle = after_label.get(..target_at).unwrap_or_default();
    let suffix = after_label
        .get(target_at.saturating_add(8)..)
        .unwrap_or_default();
    let Some(rest) = label
        .strip_prefix(prefix)
        .and_then(|s| s.strip_suffix(suffix))
    else {
        return (label, None);
    };
    match rest.split_once(middle) {
        Some((base, target)) if !target.is_empty() => (base, Some(target)),
        _ => (label, None),
    }
}

fn is_question_control(target: &FocusTarget) -> bool {
    matches!(
        target,
        FocusTarget::Choice(_)
            | FocusTarget::SequenceMove { .. }
            | FocusTarget::FillIn
            | FocusTarget::MatchRow(_)
            | FocusTarget::Hotspot
            | FocusTarget::DragItem(_)
    )
}

/// Is this object non-interactive content on this frame?
fn content_object(list: &[FocusTarget], object: &Object) -> bool {
    if object.decorative {
        return false;
    }
    let target = FocusTarget::Object(object.id.clone());
    !list.contains(&target) && !list.contains(&FocusTarget::DragItem(object.id.clone()))
}

fn content_node(
    player: &Player<'_>,
    slide: &Slide,
    items: &[DrawItem],
    object: &Object,
    key: String,
) -> Option<AccessNode> {
    if object.decorative {
        return None;
    }
    let item = object_item(items, object).or_else(|| {
        slide
            .question
            .as_ref()
            .and_then(|q| q.hotspot_image.as_deref())
            .filter(|id| *id == object.id)
            .and_then(|id| hotspot_item(items, id))
    })?;
    let (role, label) = match &object.kind {
        ObjectKind::Text { .. } => {
            let text = item_text(item).unwrap_or_default();
            if text.is_empty() {
                return None;
            }
            (AccessRole::Text, text)
        }
        ObjectKind::Image { .. } | ObjectKind::Lottie { .. } | ObjectKind::Web { .. } => {
            let label = object
                .description
                .as_deref()
                .filter(|s| !s.is_empty())
                .map(|s| player.display_text(s))
                .or_else(|| {
                    item_alt(item)
                        .filter(|s| !s.is_empty())
                        .map(|s| player.display_text(&s))
                })
                .unwrap_or_default();
            (AccessRole::Image, label)
        }
        ObjectKind::Shape { .. } => {
            let text = object
                .description
                .as_deref()
                .map(|s| player.display_text(s))
                .or_else(|| item_text(item))
                .filter(|s| !s.is_empty());
            (AccessRole::Text, text?)
        }
        ObjectKind::Button { .. } => {
            let text = item_text(item)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    object
                        .description
                        .as_deref()
                        .map(|s| player.display_text(s))
                })
                .filter(|s| !s.is_empty());
            (AccessRole::Text, text?)
        }
        // A path is a shape in the reading order: its description is its only
        // name, and with none it is decorative like an unnamed shape.
        ObjectKind::Path { .. } => {
            let text = object
                .description
                .as_deref()
                .map(|s| player.display_text(s))
                .filter(|s| !s.is_empty());
            (AccessRole::Text, text?)
        }
        ObjectKind::Table { .. } => {
            let label = object
                .description
                .as_deref()
                .filter(|text| !text.trim().is_empty())
                .map(|text| player.display_text(text))
                .or_else(|| (!object.name.trim().is_empty()).then(|| object.name.clone()))
                .unwrap_or_else(|| "Table".to_owned());
            (AccessRole::Table, label)
        }
        ObjectKind::TextEntry { .. } => {
            let label = input_node_label(player, object, "Text field");
            (
                if matches!(
                    &object.kind,
                    ObjectKind::TextEntry {
                        multiline: true,
                        ..
                    }
                ) {
                    AccessRole::TextArea
                } else {
                    AccessRole::TextBox
                },
                label,
            )
        }
        ObjectKind::Slider { .. } | ObjectKind::Dial { .. } => (
            AccessRole::Slider,
            input_node_label(player, object, "Slider"),
        ),
        ObjectKind::Checkbox { .. } => (
            AccessRole::Checkbox,
            input_node_label(player, object, "Checkbox"),
        ),
        ObjectKind::RadioGroup { .. } => (
            AccessRole::RadioGroup,
            input_node_label(player, object, "Radio group"),
        ),
        ObjectKind::Timer { .. } => (AccessRole::Timer, input_node_label(player, object, "Timer")),
        ObjectKind::ProgressMeter { .. } => (
            AccessRole::ProgressBar,
            input_node_label(player, object, "Progress meter"),
        ),
        ObjectKind::Marker { .. } | ObjectKind::ScrollPanel { .. } => return None,
        ObjectKind::Audio { .. } | ObjectKind::Video { .. } => return None,
    };
    let (value, options) = match &object.kind {
        ObjectKind::Timer { format, .. } => {
            let remaining = player.timer_remaining_ms(&object.id).unwrap_or(0);
            let seconds = coursecraft_course::timer_remaining_seconds(remaining);
            (
                Some(coursecraft_course::format_timer_text(remaining, *format)),
                vec![("seconds".to_owned(), seconds.to_string())],
            )
        }
        ObjectKind::ProgressMeter {
            variable, min, max, ..
        } => {
            let raw = player
                .variable(variable)
                .and_then(|value| match value {
                    Value::Number(number) => Some(*number),
                    _ => None,
                })
                .unwrap_or(*min);
            let now = if raw.is_finite() {
                raw.clamp(min.min(*max), min.max(*max))
            } else {
                *min
            };
            let percent = if max > min {
                ((now - min) / (max - min) * 100.0)
                    .round()
                    .clamp(0.0, 100.0)
            } else {
                0.0
            };
            (
                Some(format!("{percent:.0}%")),
                vec![
                    ("min".to_owned(), format_input_number(*min)),
                    ("max".to_owned(), format_input_number(*max)),
                    ("now".to_owned(), format_input_number(now)),
                ],
            )
        }
        _ => (None, Vec::new()),
    };
    Some(AccessNode {
        key,
        role,
        label,
        value,
        options,
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: None,
        rect: item.rect,
        screen: false,
    })
}

/// Emit a table node, then its rows and cells in reading order.
fn push_table_nodes(
    player: &Player<'_>,
    slide: &Slide,
    items: &[DrawItem],
    object: &Object,
    key: &str,
    nodes: &mut Vec<AccessNode>,
) {
    let ObjectKind::Table {
        columns,
        rows,
        header_row,
        ..
    } = &object.kind
    else {
        return;
    };
    let Some(item) = object_item(items, object) else {
        return;
    };
    let label = object
        .description
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| player.display_text(text))
        .or_else(|| (!object.name.trim().is_empty()).then(|| object.name.clone()))
        .unwrap_or_else(|| "Table".to_owned());
    nodes.push(AccessNode {
        key: format!("{key}:table"),
        role: AccessRole::Table,
        label,
        value: None,
        options: Vec::new(),
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: None,
        rect: item.rect,
        screen: false,
    });
    let col_offsets = coursecraft_course::table_fraction_offsets(columns);
    let heights: Vec<f32> = rows.iter().map(|row| row.height).collect();
    let row_offsets = coursecraft_course::table_fraction_offsets(&heights);
    let owners = coursecraft_course::table_cell_owners(rows, columns.len());
    for (row_index, row) in rows.iter().enumerate() {
        nodes.push(AccessNode {
            key: format!("{key}:row:{row_index}"),
            role: AccessRole::Row,
            label: String::new(),
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: None,
            rect: row_rect(item.rect, &row_offsets, row_index),
            screen: false,
        });
        for (column, cell) in row.cells.iter().enumerate() {
            let owner = owners
                .get(row_index)
                .and_then(|line| line.get(column))
                .copied()
                .flatten();
            if owner != Some((row_index, column)) {
                continue;
            }
            let text = if cell.runs.is_empty() {
                cell.text.clone()
            } else {
                cell.runs.iter().map(|run| run.text.as_str()).collect()
            };
            let role = if *header_row && row_index == 0 {
                AccessRole::ColumnHeader
            } else {
                AccessRole::Cell
            };
            nodes.push(AccessNode {
                key: format!("{key}:cell:{row_index}:{column}"),
                role,
                label: player.display_text(&text),
                value: None,
                options: Vec::new(),
                checked: None,
                pressed: None,
                expanded: None,
                hint: None,
                target: None,
                rect: cell_rect(item.rect, &col_offsets, &row_offsets, row_index, column),
                screen: false,
            });
        }
    }
    let _ = slide;
}

fn cell_rect(
    frame: Rect,
    columns: &[(f32, f32)],
    rows: &[(f32, f32)],
    row: usize,
    column: usize,
) -> Rect {
    let Some((left, right)) = columns.get(column) else {
        return frame;
    };
    let Some((top, bottom)) = rows.get(row) else {
        return frame;
    };
    Rect::from_min_max(
        egui::pos2(
            frame.left() + left * frame.width(),
            frame.top() + top * frame.height(),
        ),
        egui::pos2(
            frame.left() + right * frame.width(),
            frame.top() + bottom * frame.height(),
        ),
    )
}

fn row_rect(frame: Rect, rows: &[(f32, f32)], row: usize) -> Rect {
    let Some((top, bottom)) = rows.get(row) else {
        return frame;
    };
    Rect::from_min_max(
        egui::pos2(frame.left(), frame.top() + top * frame.height()),
        egui::pos2(frame.right(), frame.top() + bottom * frame.height()),
    )
}

fn format_input_number(value: f64) -> String {
    if !value.is_finite() {
        return "0".to_owned();
    }
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        let text = format!("{value:.2}");
        text.trim_end_matches('0').trim_end_matches('.').to_owned()
    }
}

/// The accessible node for an input object. Options carry the slider's range as
/// `("min"|"max"|"now", value)` so the web layer can write `aria-value*`.
fn input_access_node(
    player: &Player<'_>,
    slide: &Slide,
    items: &[DrawItem],
    object: &Object,
    target: &FocusTarget,
) -> Option<AccessNode> {
    let item = object_item(items, object);
    let rect = item.map_or_else(|| object_rect(object), |i| i.rect);
    let mut node = AccessNode {
        key: format!("object:{}:{}", slide.id, object.id),
        role: AccessRole::Button,
        label: input_node_label(player, object, "Input"),
        value: None,
        options: Vec::new(),
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: Some(target.clone()),
        rect,
        screen: false,
    };
    match &object.kind {
        ObjectKind::TextEntry {
            variable,
            multiline,
            ..
        } => {
            node.role = if *multiline {
                AccessRole::TextArea
            } else {
                AccessRole::TextBox
            };
            node.value = match player.variable(variable) {
                Some(Value::Text(text)) => Some(text.clone()),
                _ => Some(String::new()),
            };
        }
        ObjectKind::Slider {
            variable, min, max, ..
        }
        | ObjectKind::Dial {
            variable, min, max, ..
        } => {
            node.role = AccessRole::Slider;
            let now = match player.variable(variable) {
                Some(Value::Number(number)) => *number,
                _ => *min,
            };
            node.value = Some(format_input_number(now));
            node.options = vec![
                ("min".to_owned(), format_input_number(*min)),
                ("max".to_owned(), format_input_number(*max)),
                ("now".to_owned(), format_input_number(now)),
            ];
        }
        ObjectKind::Checkbox { variable, .. } => {
            node.role = AccessRole::Checkbox;
            node.checked = Some(matches!(player.variable(variable), Some(Value::Bool(true))));
        }
        ObjectKind::RadioGroup { variable, options } => {
            node.role = AccessRole::RadioGroup;
            node.options = options
                .iter()
                .map(|option| (option.value.clone(), player.display_text(&option.label)))
                .collect();
            node.value = match player.variable(variable) {
                Some(Value::Text(text)) => Some(text.clone()),
                _ => None,
            };
        }
        _ => return None,
    }
    Some(node)
}

/// The card text an open marker exposes: its title, then its body.
fn marker_card_text(object: &Object) -> Option<String> {
    let ObjectKind::Marker {
        title, body, runs, ..
    } = &object.kind
    else {
        return None;
    };
    let body = if runs.is_empty() {
        body.clone()
    } else {
        runs.iter().map(|run| run.text.as_str()).collect()
    };
    let mut lines = Vec::new();
    if !title.trim().is_empty() {
        lines.push(title.clone());
    }
    if !body.trim().is_empty() {
        lines.push(body);
    }
    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn push_marker_card(
    player: &Player<'_>,
    slide: &Slide,
    object: &Object,
    nodes: &mut Vec<AccessNode>,
) {
    if player.marker_open() != Some(object.id.as_str()) {
        return;
    }
    let Some(text) = marker_card_text(object) else {
        return;
    };
    let key = format!("marker-card:{}:{}", slide.id, object.id);
    if nodes.iter().any(|node| node.key == key) {
        return;
    }
    nodes.push(AccessNode {
        key,
        role: AccessRole::Text,
        label: player.display_text(&text),
        value: None,
        options: Vec::new(),
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: None,
        rect: object_rect(object),
        screen: false,
    });
}

#[allow(clippy::cognitive_complexity)]
fn target_node(
    player: &Player<'_>,
    frame: &PlayerFrame,
    items: &[DrawItem],
    slide: &Slide,
    target: &FocusTarget,
) -> Option<AccessNode> {
    match target {
        FocusTarget::Link {
            object,
            index,
            link,
        } => {
            let item = slide
                .object(object)
                .and_then(|(obj, _)| object_item(items, obj));
            let rect = if let Some(item) = item {
                item.links
                    .get(*index)
                    .map(|link| link_rect(item.rect, link))
                    .unwrap_or(item.rect)
            } else {
                slide.object(object).map(|(obj, _)| object_rect(obj))?
            };
            Some(AccessNode {
                key: format!("link:{}:{}:{}", slide.id, object, index),
                role: AccessRole::Link,
                label: link_label(player, link),
                value: None,
                options: Vec::new(),
                checked: None,
                pressed: None,
                expanded: None,
                hint: None,
                target: Some(target.clone()),
                rect,
                screen: false,
            })
        }
        FocusTarget::Object(id) => {
            let (object, _) = slide.object(id)?;
            if object.kind.input_kind().is_some() {
                return input_access_node(player, slide, items, object, target);
            }
            if matches!(object.kind, ObjectKind::Marker { .. }) {
                let label = object
                    .description
                    .as_deref()
                    .filter(|text| !text.trim().is_empty())
                    .map(|text| player.display_text(text))
                    .or_else(|| object_label(player, slide, id).filter(|label| !label.is_empty()))
                    .unwrap_or_else(|| "Marker".to_owned());
                let item = object_item(items, object);
                return Some(AccessNode {
                    key: format!("object:{}:{}", slide.id, object.id),
                    role: AccessRole::Button,
                    label,
                    value: None,
                    options: Vec::new(),
                    checked: None,
                    pressed: None,
                    expanded: Some(player.marker_open() == Some(object.id.as_str())),
                    hint: None,
                    target: Some(target.clone()),
                    rect: item.map_or_else(|| object_rect(object), |i| i.rect),
                    screen: false,
                });
            }
            let item = object_item(items, object);
            let mut label = item
                .and_then(item_text)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    item.and_then(item_alt)
                        .filter(|s| !s.is_empty())
                        .map(|s| player.display_text(&s))
                })
                .or_else(|| {
                    object
                        .description
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|s| player.display_text(s))
                })
                .or_else(|| (!object.name.is_empty()).then(|| player.display_text(&object.name)))
                .unwrap_or_else(|| object.id.clone());
            let checked =
                object.states.iter().any(|s| s.name == "selected").then(|| {
                    player.object_state(id).unwrap_or(&object.initial_state) == "selected"
                });
            let drag = frame.focus().drag_visual();
            let held = drag.as_ref().is_some_and(|d| &d.item == id);
            let target_id = if object.drag.is_some() {
                if held {
                    drag.as_ref().and_then(|drag| drag.target.clone())
                } else {
                    player.dropped_target(id)
                }
            } else {
                None
            };
            if let Some(target) = target_id
                && let Some(target_label) = object_label(player, slide, &target)
            {
                label = fill(
                    player.course().chrome_string("dragOnTarget"),
                    &[("label", &label), ("target", &target_label)],
                );
            }
            let hint = if object.drag.is_some() {
                Some(player.course().chrome_string("dragHint").to_owned())
            } else {
                None
            };
            Some(AccessNode {
                key: if object.drag.is_some() {
                    format!("drag:{}:{}", slide.id, object.id)
                } else {
                    format!("object:{}:{}", slide.id, object.id)
                },
                role: AccessRole::Button,
                label,
                value: None,
                options: Vec::new(),
                checked,
                pressed: if object.drag.is_some() {
                    Some(held)
                } else {
                    None
                },
                expanded: None,
                hint,
                target: Some(target.clone()),
                rect: item.map_or_else(|| object_rect(object), |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::Choice(id) => {
            let question = slide.question.as_ref()?;
            let choice = question.choices.iter().find(|c| &c.id == id)?;
            let role = if question.kind == QuestionKind::MultipleResponse {
                AccessRole::Checkbox
            } else {
                AccessRole::Radio
            };
            let item = items.iter().rev().find(|item| match &item.hit {
                Hit::Choice(hit) | Hit::QuestionChrome(hit) => hit == id,
                _ => false,
            });
            let mark = item.and_then(|item| match &item.visual {
                Visual::ChoiceRow { mark, .. } => *mark,
                _ => None,
            });
            Some(AccessNode {
                key: format!("choice:{}:{}", slide.id, choice.id),
                role,
                label: player.display_text(&choice.text),
                value: mark_value(player, mark),
                options: Vec::new(),
                checked: Some(player.state().selected.contains(&choice.id)),
                pressed: None,
                expanded: None,
                hint: None,
                target: Some(target.clone()),
                rect: item.map_or(Rect::NOTHING, |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::SequenceMove { choice, up } => {
            let question = slide.question.as_ref()?;
            let order = sequence_order(player, question);
            let index = order.iter().position(|id| id == choice)?;
            let text = question
                .choices
                .iter()
                .find(|c| &c.id == choice)
                .map(|c| player.display_text(&c.text))
                .unwrap_or_else(|| choice.clone());
            let direction = if *up {
                player.course().chrome_string("directionUp")
            } else {
                player.course().chrome_string("directionDown")
            };
            let to = if *up {
                index.checked_sub(1)?
            } else {
                index.checked_add(1).filter(|i| *i < order.len())?
            };
            let item = items.iter().rev().find(|item| match &item.hit {
                Hit::SequenceMove {
                    choice: hit_choice,
                    to: hit_to,
                } => hit_choice == choice && *hit_to == to,
                _ => false,
            });
            Some(AccessNode {
                key: format!(
                    "sequence:{}:{}:{}",
                    slide.id,
                    choice,
                    if *up { "up" } else { "down" }
                ),
                role: AccessRole::Button,
                label: fill(
                    player.course().chrome_string("sequenceMove"),
                    &[
                        ("text", &text),
                        ("direction", direction),
                        ("n", &index.saturating_add(1).to_string()),
                        ("m", &order.len().to_string()),
                    ],
                ),
                value: None,
                options: Vec::new(),
                checked: None,
                pressed: None,
                expanded: None,
                hint: None,
                target: Some(target.clone()),
                rect: item.map_or(Rect::NOTHING, |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::FillIn => {
            let question = slide.question.as_ref()?;
            let item = items.iter().find(|item| match &item.hit {
                Hit::QuestionChrome(id) => id == "fill-in",
                _ => false,
            });
            let mark = item.and_then(|item| match &item.visual {
                Visual::FillIn { mark, .. } => *mark,
                _ => None,
            });
            Some(AccessNode {
                key: format!("fill-in:{}", slide.id),
                role: if question.kind == QuestionKind::Essay {
                    AccessRole::TextArea
                } else {
                    AccessRole::TextBox
                },
                label: player.display_text(&question.prompt),
                value: mark_value(player, mark).or_else(|| Some(player.text_entry().to_owned())),
                options: Vec::new(),
                checked: None,
                pressed: None,
                expanded: None,
                hint: None,
                target: Some(target.clone()),
                rect: item.map_or(Rect::NOTHING, |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::MatchRow(left) => {
            let question = slide.question.as_ref()?;
            let pair = question.pairs.iter().find(|p| &p.id == left)?;
            let mut options = vec![(
                String::new(),
                player
                    .course()
                    .chrome_string("choosePlaceholder")
                    .to_owned(),
            )];
            for option in player.matching_options() {
                let label = question
                    .pairs
                    .iter()
                    .find(|p| p.id == option)
                    .map(|p| player.display_text(&p.right))
                    .unwrap_or_default();
                options.push((option, label));
            }
            let item = items.iter().find(|item| match &item.hit {
                Hit::QuestionChrome(id) => id == left,
                _ => false,
            });
            let mark = item.and_then(|item| match &item.visual {
                Visual::MatchRow { mark, .. } => *mark,
                _ => None,
            });
            Some(AccessNode {
                key: format!("match:{}:{}", slide.id, pair.id),
                role: AccessRole::Select,
                label: player.display_text(&pair.left),
                value: mark_value(player, mark)
                    .or_else(|| Some(player.match_for(left).unwrap_or_default().to_owned())),
                options,
                checked: None,
                pressed: None,
                expanded: None,
                hint: None,
                target: Some(target.clone()),
                rect: item.map_or(Rect::NOTHING, |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::Hotspot => {
            let question = slide.question.as_ref()?;
            let item = question
                .hotspot_image
                .as_deref()
                .and_then(|id| hotspot_item(items, id));
            Some(AccessNode {
                key: format!("hotspot:{}", slide.id),
                role: AccessRole::Slider,
                label: player.course().chrome_string("placeMarker").to_owned(),
                value: Some(fill(
                    player.course().chrome_string("markersPlaced"),
                    &[("n", &player.state().hotspot_markers.len().to_string())],
                )),
                options: Vec::new(),
                checked: None,
                pressed: None,
                expanded: None,
                hint: Some(player.course().chrome_string("placeMarkerHint").to_owned()),
                target: Some(target.clone()),
                rect: item.map_or(Rect::NOTHING, |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::DragItem(id) => {
            let (object, _) = slide.object(id)?;
            let item = object_item(items, object);
            let mut label = item
                .and_then(item_text)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    object
                        .description
                        .as_deref()
                        .filter(|s| !s.is_empty())
                        .map(|s| player.display_text(s))
                })
                .or_else(|| (!object.name.is_empty()).then(|| player.display_text(&object.name)))
                .unwrap_or_else(|| object.id.clone());
            let drag = frame.focus().drag_visual();
            let held = drag.as_ref().is_some_and(|drag| &drag.item == id);
            let target_id = if held {
                drag.as_ref().and_then(|drag| drag.target.as_deref())
            } else {
                player.state().drops.get(id).map(String::as_str)
            };
            if let Some(target) = target_id
                && let Some(target_label) = object_label(player, slide, target)
            {
                label = fill(
                    player.course().chrome_string("dragOnTarget"),
                    &[("label", &label), ("target", &target_label)],
                );
            }
            let mark = item.and_then(|item| item.mark);
            Some(AccessNode {
                key: format!("drag:{}:{}", slide.id, object.id),
                role: AccessRole::Button,
                label,
                value: mark_value(player, mark),
                options: Vec::new(),
                checked: None,
                pressed: Some(held),
                expanded: None,
                hint: Some(player.course().chrome_string("dragHint").to_owned()),
                target: Some(target.clone()),
                rect: item.map_or_else(|| object_rect(object), |i| i.rect),
                screen: false,
            })
        }
        FocusTarget::Chrome(button) => {
            let course = player.course();
            let (label, rect, role, value, pressed, hint) = match button {
                ChromeButton::DismissNotice => (
                    course.chrome_string("dismissNotice").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.notice_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::ExitReview => (
                    course.chrome_string("exitReview").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.review_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::PlayPause => {
                    let label = if crate::chrome::at_slide_end(player) {
                        course.chrome_string("replaySlide")
                    } else if player.player_paused() {
                        course.chrome_string("play")
                    } else {
                        course.chrome_string("pause")
                    };
                    (
                        label.to_owned(),
                        frame.chrome_rect(button).unwrap_or(Rect::NOTHING),
                        AccessRole::Button,
                        None,
                        Some(false),
                        None,
                    )
                }
                ChromeButton::Replay => (
                    course.chrome_string("replaySlide").to_owned(),
                    frame.chrome_rect(button).unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::Seek => {
                    let info = player.seek_info();
                    (
                        course.chrome_string("slidePosition").to_owned(),
                        frame.chrome_rect(button).unwrap_or(Rect::NOTHING),
                        AccessRole::Slider,
                        Some(format!(
                            "{} / {}",
                            crate::chrome::format_ms(info.elapsed_ms),
                            crate::chrome::format_ms(info.duration_ms)
                        )),
                        None,
                        None,
                    )
                }
                ChromeButton::Mute => {
                    let muted = frame.volume() <= 0.0;
                    (
                        if muted {
                            course.chrome_string("unmute").to_owned()
                        } else {
                            course.chrome_string("mute").to_owned()
                        },
                        frame.chrome_rect(button).unwrap_or(Rect::NOTHING),
                        AccessRole::Button,
                        None,
                        Some(muted),
                        None,
                    )
                }
                ChromeButton::Volume => (
                    course.chrome_string("volume").to_owned(),
                    frame.chrome_rect(button).unwrap_or(Rect::NOTHING),
                    AccessRole::Slider,
                    Some(format!("{}%", (frame.volume() * 100.0).round() as i32)),
                    None,
                    None,
                ),
                ChromeButton::Captions => (
                    course.chrome_string("captions").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    Some(frame.captions_on()),
                    None,
                ),
                ChromeButton::Outline => (
                    course.chrome_string("courseOutline").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::Previous => (
                    course.chrome_string("previousSlide").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::Next => (
                    course.chrome_string("nextSlide").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    (!player.can_next()).then(|| course.chrome_string("locked").to_owned()),
                    None,
                    None,
                ),
                ChromeButton::Speed => (
                    course.chrome_string("playbackSpeed").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    Some(coursecraft_course::fill(
                        course.chrome_string("speedValue"),
                        &[("n", &format!("{}", player.speed()))],
                    )),
                    None,
                    None,
                ),
                ChromeButton::Fullscreen => (
                    course.chrome_string("fullScreen").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.strip_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    Some(frame.fullscreen),
                    None,
                ),
                ChromeButton::DrawerClose => (
                    course.chrome_string("closeOutline").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
                ChromeButton::DrawerRow(slide_id) => {
                    let label = player
                        .play_order()
                        .iter()
                        .find(|s| &s.id == slide_id)
                        .map(|s| player.display_text(&s.title))
                        .unwrap_or_else(|| slide_id.clone());
                    let rect = frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING);
                    let current = player.current_slide().is_some_and(|s| &s.id == slide_id);
                    let locked = !current && !player.can_go_to(slide_id);
                    (
                        label,
                        rect,
                        AccessRole::Button,
                        locked.then(|| course.chrome_string("locked").to_owned()),
                        None,
                        None,
                    )
                }
                ChromeButton::DrawerTab(tab) => (
                    course.chrome_string(tab.label_key()).to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Tab,
                    None,
                    Some(frame.focus().drawer_view().effective_tab(course) == *tab),
                    None,
                ),
                ChromeButton::ResourceRow(id) => (
                    course
                        .resources
                        .iter()
                        .find(|r| &r.id == id)
                        .map_or_else(|| id.clone(), |r| r.title.clone()),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Link,
                    None,
                    None,
                    None,
                ),
                ChromeButton::GlossarySearch => (
                    course.chrome_string("glossarySearch").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::TextBox,
                    Some(frame.focus().drawer_view().query.clone()),
                    None,
                    None,
                ),
                ChromeButton::GlossaryTerm(id) => {
                    let entry = course.glossary.iter().find(|g| &g.id == id);
                    let open = frame.focus().drawer_view().selected.as_deref() == Some(id.as_str());
                    (
                        entry.map_or_else(|| id.clone(), |g| g.term.clone()),
                        frame
                            .chrome_rect(button)
                            .or_else(|| frame.drawer_rect())
                            .unwrap_or(Rect::NOTHING),
                        AccessRole::ListItem,
                        open.then(|| entry.map(|g| g.definition.clone())).flatten(),
                        None,
                        None,
                    )
                }
                ChromeButton::TranscriptText => (
                    player
                        .current_slide()
                        .map(|slide| slide.notes.clone())
                        .filter(|notes| !notes.trim().is_empty())
                        .unwrap_or_else(|| course.chrome_string("transcriptEmpty").to_owned()),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.drawer_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Text,
                    None,
                    None,
                    None,
                ),
                ChromeButton::CloseLightbox => (
                    course.chrome_string("closeLightbox").to_owned(),
                    frame
                        .chrome_rect(button)
                        .or_else(|| frame.lightbox_close_rect())
                        .unwrap_or(Rect::NOTHING),
                    AccessRole::Button,
                    None,
                    None,
                    None,
                ),
            };
            Some(AccessNode {
                key: format!("chrome:{}", chrome_key(button)),
                role,
                label,
                value,
                options: Vec::new(),
                checked: None,
                pressed,
                expanded: None,
                hint,
                target: Some(target.clone()),
                rect,
                screen: true,
            })
        }
    }
}

fn chrome_key(button: &ChromeButton) -> String {
    match button {
        ChromeButton::DismissNotice => "dismiss-notice".to_owned(),
        ChromeButton::ExitReview => "exit-review".to_owned(),
        ChromeButton::PlayPause => "play-pause".to_owned(),
        ChromeButton::Replay => "replay".to_owned(),
        ChromeButton::Seek => "seek".to_owned(),
        ChromeButton::Mute => "mute".to_owned(),
        ChromeButton::Volume => "volume".to_owned(),
        ChromeButton::Captions => "captions".to_owned(),
        ChromeButton::Outline => "outline".to_owned(),
        ChromeButton::Previous => "previous".to_owned(),
        ChromeButton::Next => "next".to_owned(),
        ChromeButton::Speed => "speed".to_owned(),
        ChromeButton::DrawerTab(tab) => format!("drawer-tab:{}", tab.label_key()),
        ChromeButton::ResourceRow(id) => format!("resource:{id}"),
        ChromeButton::GlossarySearch => "glossary-search".to_owned(),
        ChromeButton::GlossaryTerm(id) => format!("glossary-term:{id}"),
        ChromeButton::TranscriptText => "transcript".to_owned(),
        ChromeButton::Fullscreen => "fullscreen".to_owned(),
        ChromeButton::DrawerClose => "drawer-close".to_owned(),
        ChromeButton::DrawerRow(id) => format!("drawer-row:{id}"),
        ChromeButton::CloseLightbox => "close-lightbox".to_owned(),
    }
}

fn mark_value(player: &Player<'_>, mark: Option<ReviewMark>) -> Option<String> {
    mark.map(|mark| {
        player
            .course()
            .chrome_string(match mark {
                ReviewMark::Correct => "markCorrect",
                ReviewMark::Incorrect => "markIncorrect",
                ReviewMark::MissedCorrect => "markMissed",
            })
            .to_owned()
    })
}

fn object_item<'a>(items: &'a [DrawItem], object: &Object) -> Option<&'a DrawItem> {
    items.iter().rev().find(|item| match &item.hit {
        Hit::Object(id) | Hit::DragItem(id) | Hit::QuestionChrome(id) => id == &object.id,
        _ => false,
    })
}

fn hotspot_item<'a>(items: &'a [DrawItem], id: &str) -> Option<&'a DrawItem> {
    items.iter().rev().find(|item| match &item.hit {
        Hit::HotspotImage => true,
        Hit::QuestionChrome(hit) => hit == "hotspot" || hit == id,
        Hit::Object(hit) | Hit::DragItem(hit) => hit == id,
        _ => false,
    })
}

fn item_text(item: &DrawItem) -> Option<String> {
    match &item.visual {
        Visual::Text { runs, .. } => Some(runs.iter().map(|r| r.text.as_str()).collect()),
        Visual::Button { label, .. } => Some(label.clone()),
        Visual::Prompt { text } | Visual::ChoiceRow { text, .. } => Some(text.clone()),
        Visual::SequenceRow { text, .. } => Some(text.clone()),
        Visual::MatchRow { left, .. } => Some(left.clone()),
        Visual::CorrectAnswer { text } => Some(text.clone()),
        Visual::Table(table) => {
            let mut lines = Vec::new();
            for row in &table.rows {
                for cell in &row.cells {
                    let text: String = cell.text.runs.iter().map(|run| run.text.as_str()).collect();
                    if !text.is_empty() {
                        lines.push(text);
                    }
                }
            }
            (!lines.is_empty()).then(|| lines.join("\n"))
        }
        Visual::TextEntry { text, .. } => (!text.is_empty()).then(|| text.clone()),
        Visual::Checkbox { label, .. } => (!label.is_empty()).then(|| label.clone()),
        Visual::RadioGroup { options } => {
            let labels: Vec<&str> = options
                .iter()
                .map(|(label, _, _)| label.as_str())
                .filter(|label| !label.is_empty())
                .collect();
            (!labels.is_empty()).then(|| labels.join("\n"))
        }
        Visual::Slider { .. } | Visual::Dial { .. } => None,
        Visual::Timer { runs, .. } => {
            let text: String = runs.iter().map(|r| r.text.as_str()).collect();
            (!text.is_empty()).then_some(text)
        }
        Visual::ProgressMeter { percent, .. } => Some(format!("{percent}%")),
        Visual::Marker { title, .. } => (!title.is_empty()).then(|| title.clone()),
        Visual::DrawCard { text } => (!text.is_empty()).then(|| text.clone()),
        Visual::Image { .. }
        | Visual::ImagePlaceholder { .. }
        | Visual::Video { .. }
        | Visual::Web { .. }
        | Visual::Audio { .. }
        | Visual::Lottie { .. }
        | Visual::FillIn { .. }
        | Visual::SequenceButton { .. }
        | Visual::ScrollPanel { .. }
        | Visual::HotspotMarker
        | Visual::RegionOutline { .. }
        | Visual::Path(_) => None,
        Visual::Shape { text, .. } => text
            .as_ref()
            .map(|text| text.runs.iter().map(|run| run.text.as_str()).collect()),
    }
}

fn item_alt(item: &DrawItem) -> Option<String> {
    match &item.visual {
        Visual::Image { alt, .. } => Some(alt.clone()),
        Visual::Web { label } => Some(label.clone()),
        _ => None,
    }
}

fn object_rect(object: &Object) -> Rect {
    validate_frame(&object.frame).unwrap_or(Rect::NOTHING)
}

/// Add one accessible link node for every link on `object`, right after it.
fn push_link_nodes(
    player: &Player<'_>,
    slide: &Slide,
    items: &[DrawItem],
    object_id: &str,
    key_prefix: &str,
    nodes: &mut Vec<AccessNode>,
) {
    let links = player.object_links(object_id);
    if links.is_empty() {
        return;
    }
    let item = slide
        .object(object_id)
        .and_then(|(obj, _)| object_item(items, obj));
    let fallback = item
        .map(|item| item.rect)
        .or_else(|| slide.object(object_id).map(|(obj, _)| object_rect(obj)))
        .unwrap_or(Rect::NOTHING);
    for (index, link) in links.into_iter().enumerate() {
        let rect = item
            .and_then(|item| item.links.get(index).map(|link| link_rect(item.rect, link)))
            .unwrap_or(fallback);
        nodes.push(AccessNode {
            key: format!("{key_prefix}:link:{index}"),
            role: AccessRole::Link,
            label: link_label(player, &link),
            value: None,
            options: Vec::new(),
            checked: None,
            pressed: None,
            expanded: None,
            hint: None,
            target: Some(FocusTarget::Link {
                object: object_id.to_owned(),
                index,
                link,
            }),
            rect,
            screen: false,
        });
    }
}

fn link_rect(frame: Rect, link: &crate::compose::ResolvedLinkRect) -> Rect {
    let width = frame.width();
    let height = frame.height();
    Rect::from_min_max(
        egui::pos2(
            frame.min.x + link.rect.min.x * width,
            frame.min.y + link.rect.min.y * height,
        ),
        egui::pos2(
            frame.min.x + link.rect.max.x * width,
            frame.min.y + link.rect.max.y * height,
        ),
    )
}

fn link_label(player: &Player<'_>, link: &Link) -> String {
    match link {
        Link::Url { url } => url.clone(),
        Link::Slide { slide } => player
            .play_order()
            .iter()
            .find(|s| &s.id == slide)
            .map(|s| player.display_text(&s.title))
            .unwrap_or_else(|| slide.clone()),
    }
}

/// A master's text or image alternative, read before the slide's own content.
fn push_master_nodes(player: &Player<'_>, slide: &Slide, nodes: &mut Vec<AccessNode>) {
    let Some(master) = slide
        .master
        .as_deref()
        .and_then(|id| player.course().master(id))
    else {
        return;
    };
    for object in &master.objects {
        if let Some(node) = master_content_node(player, master, object) {
            nodes.push(node);
        }
    }
}

fn master_content_node(player: &Player<'_>, master: &Slide, object: &Object) -> Option<AccessNode> {
    if object.decorative {
        return None;
    }
    let (role, label) = match &object.kind {
        ObjectKind::Text { text, runs, .. } => {
            let joined: String = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|run| run.text.as_str()).collect()
            };
            if joined.trim().is_empty() {
                return None;
            }
            (AccessRole::Text, player.display_text(&joined))
        }
        ObjectKind::Image { alt, .. } => {
            let label = object
                .description
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .or(Some(alt.as_str()))
                .filter(|s| !s.trim().is_empty())
                .map(|s| player.display_text(s))
                .unwrap_or_default();
            (AccessRole::Image, label)
        }
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            let joined: String = if text.runs.is_empty() {
                text.text.clone()
            } else {
                text.runs.iter().map(|run| run.text.as_str()).collect()
            };
            let label = object
                .description
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(|s| player.display_text(s))
                .unwrap_or_else(|| player.display_text(&joined));
            if label.trim().is_empty() {
                return None;
            }
            (AccessRole::Text, label)
        }
        _ => return None,
    };
    Some(AccessNode {
        key: format!("master:{}:{}", master.id, object.id),
        role,
        label,
        value: None,
        options: Vec::new(),
        checked: None,
        pressed: None,
        expanded: None,
        hint: None,
        target: None,
        rect: object_rect(object),
        screen: false,
    })
}

fn question_prompt_rect(player: &Player<'_>) -> Rect {
    let size = player.course().slide_size;
    crate::compose::question_prompt_rect(size.w as f32, size.h as f32)
}

fn sequence_order(player: &Player<'_>, question: &coursecraft_course::Question) -> Vec<String> {
    let authored: Vec<String> = question.choices.iter().map(|c| c.id.clone()).collect();
    let order = player.sequence_order();
    if order.len() == authored.len()
        && authored.iter().all(|id| order.contains(id))
        && order.iter().all(|id| authored.contains(id))
    {
        order.to_vec()
    } else {
        authored
    }
}

/// An input's accessible name: its author description, its object name, then
/// the label or placeholder it draws.
fn input_node_label(player: &Player<'_>, object: &Object, fallback: &str) -> String {
    object
        .description
        .as_deref()
        .filter(|text| !text.trim().is_empty())
        .map(|text| player.display_text(text))
        .or_else(|| (!object.name.trim().is_empty()).then(|| player.display_text(&object.name)))
        .or_else(|| {
            object
                .kind
                .input_label()
                .filter(|text| !text.trim().is_empty())
                .map(|text| player.display_text(text))
        })
        .unwrap_or_else(|| fallback.to_owned())
}

fn object_label(player: &Player<'_>, slide: &Slide, id: &str) -> Option<String> {
    let (object, _) = slide.object(id)?;
    let text = match &object.kind {
        ObjectKind::Text { text, runs, .. } => {
            let joined: String = if runs.is_empty() {
                text.clone()
            } else {
                runs.iter().map(|r| r.text.as_str()).collect()
            };
            (!joined.is_empty()).then(|| player.display_text(&joined))
        }
        ObjectKind::Button { label, .. } => (!label.is_empty()).then(|| player.display_text(label)),
        ObjectKind::Image { alt, .. } => (!alt.is_empty()).then(|| player.display_text(alt)),
        ObjectKind::Path { .. } => object
            .description
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| player.display_text(s)),
        ObjectKind::Shape {
            text: Some(text), ..
        } => {
            let joined: String = if text.runs.is_empty() {
                text.text.clone()
            } else {
                text.runs.iter().map(|run| run.text.as_str()).collect()
            };
            (!joined.is_empty()).then(|| player.display_text(&joined))
        }
        ObjectKind::Table { rows, .. } => {
            let mut lines = Vec::new();
            for row in rows {
                for cell in &row.cells {
                    let text: String = if cell.runs.is_empty() {
                        cell.text.clone()
                    } else {
                        cell.runs.iter().map(|run| run.text.as_str()).collect()
                    };
                    if !text.is_empty() {
                        lines.push(player.display_text(&text));
                    }
                }
            }
            (!lines.is_empty()).then(|| lines.join("\n"))
        }
        ObjectKind::TextEntry { placeholder, .. } => {
            (!placeholder.is_empty()).then(|| player.display_text(placeholder))
        }
        ObjectKind::Checkbox { label, .. } => {
            (!label.is_empty()).then(|| player.display_text(label))
        }
        ObjectKind::RadioGroup { .. } | ObjectKind::Slider { .. } | ObjectKind::Dial { .. } => None,
        ObjectKind::Marker { title, .. } => (!title.is_empty()).then(|| player.display_text(title)),
        ObjectKind::Timer { .. } | ObjectKind::ProgressMeter { .. } => None,
        ObjectKind::Shape { text: None, .. }
        | ObjectKind::Audio { .. }
        | ObjectKind::Video { .. }
        | ObjectKind::ScrollPanel { .. }
        | ObjectKind::Web { .. }
        | ObjectKind::Lottie { .. } => None,
    };
    text.or_else(|| {
        object
            .description
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(|s| player.display_text(s))
    })
    .or_else(|| (!object.name.is_empty()).then(|| player.display_text(&object.name)))
    .or_else(|| Some(object.id.clone()))
}

#[cfg(test)]
mod tests;
