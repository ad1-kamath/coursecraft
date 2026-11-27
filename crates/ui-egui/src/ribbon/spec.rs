//! The ribbon as data: every tab, group and control in order (docs/ui/ribbon.md).
//!
//! The drawing code in `ribbon/mod.rs` walks these tables, so the docs and the UI
//! stay in step: a planned control whose feature is missing (`Action::NotYet`) is
//! drawn disabled with its reason, while controls marked `Action::NotPlanned` are
//! kept here for possible future use and hidden from the UI by owner decision.
//! Every label, caption and reason here is English source text, looked up through
//! `tl!` when drawn.

use coursecraft_render::Icon;

/// One tab of the strip.
pub struct Tab {
    /// Stable id, saved as the open tab.
    pub id: &'static str,
    pub label: &'static str,
    pub when: TabWhen,
    /// The groups, left to right. Empty while the tab is still being built.
    pub groups: &'static [Group],
    /// Groups shown instead while the Course Map is open (Home only).
    pub map_groups: Option<&'static [Group]>,
}

/// When a tab's controls can be used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabWhen {
    Always,
    /// Drawn always; its controls are disabled in the Course Map.
    SlideOpen,
}

/// A group of controls with its caption under it.
pub struct Group {
    pub id: &'static str,
    pub label: &'static str,
    /// The ↘ dialog launcher in the caption.
    pub launcher: Option<Action>,
    pub items: &'static [Item],
}

impl Group {
    /// Whether the group has at least one control visible in the UI.
    #[must_use]
    pub fn has_visible_items(&self) -> bool {
        self.items.iter().any(|item| !item.is_hidden())
    }
}

/// One control.
pub struct Item {
    /// Unique across the ribbon.
    pub id: &'static str,
    pub label: &'static str,
    pub icon: Icon,
    pub kind: Kind,
    pub action: Action,
    /// One sentence of help in the tooltip; empty for none.
    pub help: &'static str,
    /// The ▾ menu of a split control.
    pub menu: Menu,
}

impl Item {
    /// Hidden from the drawn ribbon, menus, tooltips and accessibility tree.
    #[must_use]
    pub const fn is_hidden(&self) -> bool {
        self.action.is_hidden()
    }
}

/// How a control is drawn. Consecutive small kinds stack in columns of three.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    /// 32 px icon over the label.
    Large,
    /// Large, with a ▾ strip under the label for its menu.
    LargeSplit,
    /// 16 px icon beside the label.
    Small,
    /// Small, with a ▾ for its menu.
    SmallSplit,
    /// A small icon-only toggle (Bold, Align Left…); its label is the tooltip.
    Toggle,
    /// A small icon-only toggle with a ▾ strip for its menu (Numbering).
    ToggleSplit,
    /// A text field with a ▾ (font family, size), this wide.
    Combo { width: f32 },
    /// A label and a number field (X, Y, Width, Height), the field this wide.
    Number { width: f32 },
    /// A row of tiles this many wide, with a ▾ that shows them all.
    Gallery { tiles: u8 },
}

impl Kind {
    /// Drawn in a column of small controls.
    #[must_use]
    pub fn is_small(self) -> bool {
        !matches!(self, Kind::Large | Kind::LargeSplit | Kind::Gallery { .. })
    }

    /// Has a ▾ menu.
    #[must_use]
    pub fn is_split(self) -> bool {
        matches!(
            self,
            Kind::LargeSplit | Kind::SmallSplit | Kind::ToggleSplit
        )
    }
}

/// What a control does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// An engine command id, with parameters from the selection (`menu::adapter`).
    Command(&'static str),
    /// An editor-side action.
    Ui(UiAction),
    /// Drawn disabled: "Not available yet: {reason}".
    NotYet(&'static str),
    /// Kept in the spec for possible future use and hidden from the UI by owner decision.
    NotPlanned(&'static str),
    /// A text formatting control (`crate::text_format`): it acts on the text
    /// being edited or the selected text objects.
    Text(TextOp),
}

impl Action {
    /// Kept in the spec for possible future use; hidden from the ribbon, menus,
    /// tooltips and accessibility tree by owner decision.
    #[must_use]
    pub const fn is_hidden(self) -> bool {
        matches!(self, Self::NotPlanned(_))
    }
}

/// The text formatting controls of Home › Font and Home › Paragraph.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextOp {
    /// Text Styles ▾: the course's named text styles.
    TextStyles,
    /// Font ▾: the course fonts.
    Font,
    /// Font size ▾.
    Size,
    Grow,
    Shrink,
    Bold,
    Italic,
    Underline,
    Strikethrough,
    /// Clear Formatting: remove the character formatting of the text, keeping links.
    ClearFormatting,
    /// Text Shadow.
    Shadow,
    /// Change Case ▾.
    Case,
    /// Highlight ▾: the colour behind the letters.
    Highlight,
    /// Text Direction ▾.
    Direction,
    /// Character Spacing ▾.
    Spacing,
    /// Font Color ▾: the theme colours.
    Color,
    Bullets,
    Numbering,
    IndentLess,
    IndentMore,
    /// Line Spacing ▾.
    LineSpacing,
    AlignLeft,
    AlignCenter,
    AlignRight,
    Justify,
    /// Align Text ▾: top, middle, bottom.
    AlignText,
    /// Wrap text inside the box width (turns point text into area text).
    WrapText,
}

impl TextOp {
    /// Opens a ▾ menu instead of acting at once.
    #[must_use]
    pub fn has_menu(self) -> bool {
        matches!(
            self,
            TextOp::TextStyles
                | TextOp::Font
                | TextOp::Size
                | TextOp::Spacing
                | TextOp::Color
                | TextOp::LineSpacing
                | TextOp::AlignText
                | TextOp::Case
                | TextOp::Highlight
                | TextOp::Direction
                | TextOp::Numbering
        )
    }

    /// A click on the main part opens the menu too. Numbering toggles on the main part, and
    /// Font Color and Highlight apply their last colour there; their menus stay behind the ▾.
    #[must_use]
    pub fn main_opens(self) -> bool {
        self.has_menu() && !matches!(self, TextOp::Numbering | TextOp::Color | TextOp::Highlight)
    }
}

/// Editor-side actions (dialogs, views, the clipboard), run in `ribbon/mod.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiAction {
    Paste,
    /// Paste exactly as copied.
    PasteKeepSource,
    /// Paste the text of the copied objects as one plain text box.
    PasteTextOnly,
    /// Open the Paste Special window.
    PasteSpecial,
    /// Open the Focus Order window.
    FocusOrder,
    Cut,
    Copy,
    FormatPainter,
    /// Animations › Animation Painter: copy entrance, exit and emphasis onto other objects.
    AnimationPainter,
    NewSlide,
    NewScene,
    DuplicateSlide,
    NarrationFromText,
    FindReplace,
    SelectAll,
    AltText,
    Spelling,
    FormatInspector,
    Arrange,
    Player,
    Preview,
    Publish,
    // Insert
    SlideLayer,
    Picture,
    MediaLibrary,
    Shape,
    Caption,
    Video,
    Audio,
    RecordNarration,
    WebObject,
    Lottie,
    TextBox,
    /// Insert ▸ Slide Number: the slide number reference (`%Course.SlideNumber%`).
    InsertSlideNumber,
    /// Insert ▸ Slide Number ▾ "Slide number of total".
    InsertSlideNumberOfTotal,
    /// Insert ▸ Symbol: open the symbol picker.
    InsertSymbol,
    /// Insert ▸ Reference: open the variable reference picker.
    InsertReference,
    Table,
    Button,
    Slider,
    Dial,
    InputText,
    InputCheckbox,
    InputRadio,
    Marker,
    ScrollPanel,
    Trigger,
    Interaction,
    // Slides
    Question,
    ResultsSlide,
    Certificate,
    ImportPptx,
    ImportQuestions,
    ImportDocument,
    ImportOutline,
    SlideMaster,
    // View
    CourseMap,
    SlideView,
    ZoomFit,
    ZoomIn,
    ZoomOut,
    Zoom100,
    Rulers,
    Grid,
    Guides,
    SnapToGrid,
    SnapToGuides,
    /// Show or hide a dock panel.
    Panel(crate::dock::Panel),
    /// A ↘ launcher: open a panel (a settings panel at one of its sections)
    /// and bring it forward. Never closes it.
    PanelAt(crate::dock::Panel, Option<crate::panels::Section>),
    ResetPanels,
    ThemeLight,
    ThemeDark,
    ThemeSystem,
    // Design, Transitions, Animations
    SlideSize,
    Themes,
    /// Design › Fonts (the main part): the Create New Theme Fonts window.
    ThemeFonts,
    SlideInspector,
    AnimateInspector,
    AddMotionPath,
    // Help
    Guide,
    Shortcuts,
    Welcome,
    GitHub,
    ReportProblem,
    SuggestFeature,
    CheckUpdates,
    ConnectAgent,
    AiSettings,
    Automation,
    /// A control of a contextual tab (`ribbon/contextual.rs`).
    Context(ContextOp),
    /// A gallery's tiles (`ribbon/gallery.rs`).
    Gallery(Gallery),
    // Slide Master
    InsertMaster,
    DeleteMaster,
    RenameMaster,
    /// A gallery's options: effect direction, duration, apply to all.
    Effect(EffectOp),
}

/// The galleries of the Design, Transitions and Animations tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gallery {
    Themes,
    Transitions,
    Entrance,
    Exit,
    /// Emphasis: a pick adds one (it never shows as selected).
    Emphasis,
}

/// What an effect option acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Motion {
    /// The current slide's transition.
    Transition,
    /// The selected objects' entrance animation.
    Entrance,
    /// The selected objects' exit animation.
    Exit,
}

/// The options beside a gallery.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EffectOp {
    /// Effect Options ▾: the directions of the current effect.
    Options(Motion),
    /// The direction the effect comes from.
    From(Motion, coursecraft_course::Direction),
    /// Its duration, as a number field in seconds.
    Duration(Motion),
    /// The current slide's transition on every slide.
    TransitionToAll,
}

/// Which selection shows a contextual tab (contextual-tabs.md › When each one shows).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    Drawing,
    Button,
    Picture,
    Video,
    Audio,
    Table,
    Question,
}

impl Context {
    /// The coloured caption over the context's tabs.
    #[must_use]
    pub fn caption(self) -> &'static str {
        match self {
            Context::Drawing => "Drawing Tools",
            Context::Button => "Button Tools",
            Context::Picture => "Picture Tools",
            Context::Video => "Video Tools",
            Context::Audio => "Audio Tools",
            Context::Table => "Table Tools",
            Context::Question => "Question Tools",
        }
    }
}

/// One side of the selected object's frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameField {
    X,
    Y,
    Width,
    Height,
}

/// The controls of the contextual tabs that need the selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ContextOp {
    /// A number field of the selected object's frame.
    Frame(FrameField),
    EditPoints,
    Crop,
    /// Replace the selected picture, video or audio with another file.
    ReplaceMedia,
    /// Clear the crop and go back to the picture's own size.
    ResetPicture,
    ImportCaptions,
    TableHeaderRow,
    TableInsertAbove,
    TableInsertBelow,
    TableInsertLeft,
    TableInsertRight,
    TableDeleteRow,
    TableDeleteColumn,
    TableSplit,
    /// Open the Question panel.
    QuestionPanel,
    QuestionShuffle,
}

/// A split control's ▾ menu.
#[derive(Clone, Copy)]
pub enum Menu {
    None,
    /// Entries drawn from the spec, by the same rules as controls.
    Items(&'static [Item]),
    /// A menu the editor already has, reused as is.
    Ui(UiMenu),
}

/// Menus the editor already draws elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UiMenu {
    /// New Slide ▾ (`slides::show_new_slide_popup`).
    NewSlide,
    /// Preview ▾ (this slide, selected slides, from the start).
    Preview,
    /// Arrange ▾ (the Arrange menu's commands).
    Arrange,
    /// Design › Fonts ▾ (built-in heading and body pairs, Create New Theme Fonts…).
    ThemeFonts,
    /// Shape ▾ (the shape gallery).
    Shapes,
    /// Caption ▾ (callout shapes in the shape gallery).
    Callouts,
    /// Question ▾ (the question kinds).
    Questions,
    /// Effect Options ▾ of the Transitions, Entrance or Exit gallery, built from the registries
    /// (`gallery::effect_options_menu`).
    EffectOptions(Motion),
}

/// Why a control whose feature exists is not wired to the ribbon yet (M37 step 2).
pub const WIRING_FORMAT: &str = "ribbon wiring (use the Format inspector)";
const WIRING_SLIDE: &str = "ribbon wiring (use the Slide inspector)";
const WIRING_BANKS: &str = "ribbon wiring (use Question banks in the slides panel)";

const fn item(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    kind: Kind,
    action: Action,
) -> Item {
    Item {
        id,
        label,
        icon,
        kind,
        action,
        help: "",
        menu: Menu::None,
    }
}

const fn split(
    id: &'static str,
    label: &'static str,
    icon: Icon,
    kind: Kind,
    action: Action,
    menu: Menu,
) -> Item {
    Item {
        id,
        label,
        icon,
        kind,
        action,
        help: "",
        menu,
    }
}

const fn helped(mut i: Item, help: &'static str) -> Item {
    i.help = help;
    i
}

use Action::{Command, NotPlanned, NotYet, Text, Ui};
use Kind::{Combo, Large, LargeSplit, Small, SmallSplit, Toggle, ToggleSplit};

// ---------------------------------------------------------------------------
// Shared groups
// ---------------------------------------------------------------------------

const PASTE_MENU: &[Item] = &[
    item(
        "paste.destination",
        "Use Destination Theme",
        Icon::Notepad,
        Small,
        Ui(UiAction::Paste),
    ),
    item(
        "paste.keep_source",
        "Keep Source Formatting",
        Icon::Notepad,
        Small,
        Ui(UiAction::PasteKeepSource),
    ),
    item(
        "paste.text_only",
        "Keep Text Only",
        Icon::TextT,
        Small,
        Ui(UiAction::PasteTextOnly),
    ),
    item(
        "paste.special",
        "Paste Special…",
        Icon::Notepad,
        Small,
        Ui(UiAction::PasteSpecial),
    ),
];

const COPY_MENU: &[Item] = &[
    item("copy.copy", "Copy", Icon::Copy, Small, Ui(UiAction::Copy)),
    item(
        "copy.duplicate",
        "Duplicate",
        Icon::Copy,
        Small,
        Command("object.duplicate"),
    ),
];

const PUBLISH: Group = Group {
    id: "publish",
    label: "Publish",
    launcher: None,
    items: &[
        helped(
            item(
                "publish.player",
                "Player",
                Icon::Monitor,
                Large,
                Ui(UiAction::Player),
            ),
            "Set up the player around the course.",
        ),
        helped(
            split(
                "publish.preview",
                "Preview",
                Icon::Play,
                LargeSplit,
                Ui(UiAction::Preview),
                Menu::Ui(UiMenu::Preview),
            ),
            "Play the current slide as a learner sees it.",
        ),
        helped(
            item(
                "publish.publish",
                "Publish",
                Icon::UploadSimple,
                Large,
                Ui(UiAction::Publish),
            ),
            "Publish the course for the web or an LMS.",
        ),
    ],
};

const NEW_SLIDE: Item = helped(
    split(
        "slide.new",
        "New Slide",
        Icon::PlusSquare,
        LargeSplit,
        Ui(UiAction::NewSlide),
        Menu::Ui(UiMenu::NewSlide),
    ),
    "Add a slide after the current one.",
);

// ---------------------------------------------------------------------------
// Home (Slide variant)
// ---------------------------------------------------------------------------

const HOME_SLIDE: &[Group] = &[
    Group {
        id: "clipboard",
        label: "Clipboard",
        launcher: None,
        items: &[
            split(
                "clipboard.paste",
                "Paste",
                Icon::Notepad,
                LargeSplit,
                Ui(UiAction::Paste),
                Menu::Items(PASTE_MENU),
            ),
            item(
                "clipboard.cut",
                "Cut",
                Icon::XSquare,
                Small,
                Ui(UiAction::Cut),
            ),
            split(
                "clipboard.copy",
                "Copy",
                Icon::Copy,
                SmallSplit,
                Ui(UiAction::Copy),
                Menu::Items(COPY_MENU),
            ),
            helped(
                item(
                    "clipboard.format_painter",
                    "Format Painter",
                    Icon::PaintBrush,
                    Small,
                    Ui(UiAction::FormatPainter),
                ),
                "Copy the selected object's format. Double-click to keep it on.",
            ),
            item(
                "clipboard.select_all",
                "Select All",
                Icon::Selection,
                Small,
                Ui(UiAction::SelectAll),
            ),
        ],
    },
    Group {
        id: "slide",
        label: "Slide",
        launcher: None,
        items: &[
            NEW_SLIDE,
            item(
                "slide.apply_layout",
                "Apply Layout",
                Icon::Layout,
                SmallSplit,
                NotYet(WIRING_SLIDE),
            ),
            item(
                "slide.focus_order",
                "Focus Order",
                Icon::ListNumbers,
                Small,
                Ui(UiAction::FocusOrder),
            ),
            item(
                "slide.duplicate",
                "Duplicate",
                Icon::Copy,
                Small,
                Ui(UiAction::DuplicateSlide),
            ),
        ],
    },
    Group {
        id: "font",
        label: "Font",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::Text,
            Some(crate::panels::Section::Font),
        ))),
        items: &[
            item(
                "font.text_styles",
                "Text Styles",
                Icon::TextAa,
                LargeSplit,
                Text(TextOp::TextStyles),
            ),
            // Three rows of five.
            item(
                "font.family",
                "Font",
                Icon::TextT,
                Combo { width: 128.0 },
                Text(TextOp::Font),
            ),
            item(
                "font.size",
                "Font size",
                Icon::TextT,
                Combo { width: 52.0 },
                Text(TextOp::Size),
            ),
            item(
                "font.grow",
                "Grow Font",
                Icon::CaretUp,
                Toggle,
                Text(TextOp::Grow),
            ),
            item(
                "font.shrink",
                "Shrink Font",
                Icon::CaretDown,
                Toggle,
                Text(TextOp::Shrink),
            ),
            item(
                "font.clear",
                "Clear Formatting",
                Icon::X,
                Toggle,
                Text(TextOp::ClearFormatting),
            ),
            item("font.bold", "Bold", Icon::TextB, Toggle, Text(TextOp::Bold)),
            item(
                "font.italic",
                "Italic",
                Icon::TextItalic,
                Toggle,
                Text(TextOp::Italic),
            ),
            item(
                "font.underline",
                "Underline",
                Icon::TextUnderline,
                Toggle,
                Text(TextOp::Underline),
            ),
            item(
                "font.strikethrough",
                "Strikethrough",
                Icon::Minus,
                Toggle,
                Text(TextOp::Strikethrough),
            ),
            item(
                "font.shadow",
                "Text Shadow",
                Icon::TextT,
                Toggle,
                Text(TextOp::Shadow),
            ),
            item(
                "font.spacing",
                "Character Spacing",
                Icon::ArrowsOut,
                Toggle,
                Text(TextOp::Spacing),
            ),
            item(
                "font.case",
                "Change Case",
                Icon::TextAa,
                Toggle,
                Text(TextOp::Case),
            ),
            item(
                "font.highlight",
                "Highlight",
                Icon::PaintBrush,
                ToggleSplit,
                Text(TextOp::Highlight),
            ),
            item(
                "font.color",
                "Font Color",
                Icon::Palette,
                ToggleSplit,
                Text(TextOp::Color),
            ),
            helped(
                item(
                    "font.spelling",
                    "Spelling",
                    Icon::CheckSquare,
                    Toggle,
                    Ui(UiAction::Spelling),
                ),
                "Check the spelling of the whole course.",
            ),
        ],
    },
    Group {
        id: "paragraph",
        label: "Paragraph",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::Text,
            Some(crate::panels::Section::Paragraph),
        ))),
        items: &[
            item(
                "paragraph.bullets",
                "Bullets",
                Icon::ListBullets,
                Toggle,
                Text(TextOp::Bullets),
            ),
            item(
                "paragraph.numbering",
                "Numbering",
                Icon::ListNumbers,
                ToggleSplit,
                Text(TextOp::Numbering),
            ),
            item(
                "paragraph.indent_less",
                "Decrease Indent",
                Icon::ArrowLeft,
                Toggle,
                Text(TextOp::IndentLess),
            ),
            item(
                "paragraph.indent_more",
                "Increase Indent",
                Icon::ArrowRight,
                Toggle,
                Text(TextOp::IndentMore),
            ),
            item(
                "paragraph.line_spacing",
                "Line Spacing",
                Icon::Rows,
                Toggle,
                Text(TextOp::LineSpacing),
            ),
            item(
                "paragraph.align_left",
                "Align Left",
                Icon::TextAlignLeft,
                Toggle,
                Text(TextOp::AlignLeft),
            ),
            item(
                "paragraph.align_center",
                "Center",
                Icon::TextAlignCenter,
                Toggle,
                Text(TextOp::AlignCenter),
            ),
            item(
                "paragraph.align_right",
                "Align Right",
                Icon::TextAlignRight,
                Toggle,
                Text(TextOp::AlignRight),
            ),
            item(
                "paragraph.justify",
                "Justify",
                Icon::List,
                Toggle,
                Text(TextOp::Justify),
            ),
            item(
                "paragraph.direction",
                "Text Direction",
                Icon::ArrowsDownUp,
                SmallSplit,
                Text(TextOp::Direction),
            ),
            item(
                "paragraph.align_text",
                "Align Text",
                Icon::TextColumns,
                SmallSplit,
                Text(TextOp::AlignText),
            ),
            helped(
                item(
                    "paragraph.wrap_text",
                    "Wrap Text",
                    Icon::Article,
                    Small,
                    Text(TextOp::WrapText),
                ),
                "Wrap text inside the box width. Turning this on converts point text to a fixed-width text box.",
            ),
            helped(
                item(
                    "paragraph.find",
                    "Find / Replace",
                    Icon::MagnifyingGlass,
                    Small,
                    Ui(UiAction::FindReplace),
                ),
                "Find text in the course and replace it.",
            ),
        ],
    },
    Group {
        id: "drawing",
        label: "Drawing",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::ShapeStyle,
            Some(crate::panels::Section::Fill),
        ))),
        items: &[
            split(
                "drawing.arrange",
                "Arrange",
                Icon::Stack,
                LargeSplit,
                Ui(UiAction::Arrange),
                Menu::Ui(UiMenu::Arrange),
            ),
            item(
                "drawing.quick_styles",
                "Quick Styles",
                Icon::Palette,
                LargeSplit,
                NotYet(WIRING_FORMAT),
            ),
            item(
                "drawing.fill",
                "Shape Fill",
                Icon::PaintBrush,
                Small,
                SHAPE_FILL,
            ),
            item(
                "drawing.outline",
                "Shape Outline",
                Icon::Square,
                Small,
                SHAPE_OUTLINE,
            ),
            item(
                "drawing.effect",
                "Shape Effect",
                Icon::Sparkle,
                Small,
                SHAPE_EFFECT,
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Home (Course Map variant)
// ---------------------------------------------------------------------------

const HOME_MAP: &[Group] = &[
    Group {
        id: "map.clipboard",
        label: "Clipboard",
        launcher: None,
        items: &[
            item(
                "map.paste",
                "Paste",
                Icon::Notepad,
                Large,
                Ui(UiAction::Paste),
            ),
            item("map.cut", "Cut", Icon::XSquare, Small, Ui(UiAction::Cut)),
            item("map.copy", "Copy", Icon::Copy, Small, Ui(UiAction::Copy)),
        ],
    },
    Group {
        id: "map.scenes",
        label: "Scenes",
        launcher: None,
        items: &[
            helped(
                item(
                    "map.new_scene",
                    "New Scene",
                    Icon::FilePlus,
                    Large,
                    Ui(UiAction::NewScene),
                ),
                "Add a scene with one slide.",
            ),
            item(
                "map.start_section",
                "Start Section",
                Icon::Flag,
                Small,
                NotYet("a start section other than the first"),
            ),
            item(
                "map.question_banks",
                "Question Banks",
                Icon::Cards,
                SmallSplit,
                NotYet(WIRING_BANKS),
            ),
        ],
    },
    Group {
        id: "map.slide",
        label: "Slide",
        launcher: None,
        items: &[
            Item {
                id: "map.new_slide",
                ..NEW_SLIDE
            },
            item(
                "map.apply_layout",
                "Apply Layout",
                Icon::Layout,
                SmallSplit,
                NotYet(WIRING_SLIDE),
            ),
            item(
                "map.reset",
                "Reset",
                Icon::ArrowCounterClockwise,
                Small,
                NotYet("resetting a slide to its layout"),
            ),
            item(
                "map.duplicate",
                "Duplicate",
                Icon::Copy,
                Small,
                Ui(UiAction::DuplicateSlide),
            ),
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "map.record_screen",
                "Record Screen",
                Icon::VideoCamera,
                Large,
                NotPlanned("screen recording"),
            ),
        ],
    },
    Group {
        id: "map.publish",
        ..PUBLISH
    },
];

// ---------------------------------------------------------------------------
// Insert
// ---------------------------------------------------------------------------

const PICTURE_MENU: &[Item] = &[
    item(
        "picture.file",
        "Picture from File…",
        Icon::Image,
        Small,
        Ui(UiAction::Picture),
    ),
    item(
        "picture.library",
        "Media Library…",
        Icon::FolderOpen,
        Small,
        Ui(UiAction::MediaLibrary),
    ),
    item(
        "picture.screenshot",
        "Screenshot",
        Icon::Monitor,
        Small,
        NotYet("screenshots"),
    ),
];

const SLIDE_NUMBER_MENU: &[Item] = &[
    item(
        "slide_number.number",
        "Slide number",
        Icon::ListNumbers,
        Small,
        Ui(UiAction::InsertSlideNumber),
    ),
    item(
        "slide_number.of_total",
        "Slide number of total",
        Icon::ListNumbers,
        Small,
        Ui(UiAction::InsertSlideNumberOfTotal),
    ),
];

const VIDEO_MENU: &[Item] = &[
    item(
        "video.file",
        "Video from File…",
        Icon::FilmStrip,
        Small,
        Ui(UiAction::Video),
    ),
    item(
        "video.website",
        "Video from Website…",
        Icon::Link,
        Small,
        Ui(UiAction::WebObject),
    ),
    item(
        "video.webcam",
        "Record Webcam",
        Icon::VideoCamera,
        Small,
        NotYet("webcam recording"),
    ),
    item(
        "video.library",
        "Media Library…",
        Icon::FolderOpen,
        Small,
        Ui(UiAction::MediaLibrary),
    ),
];

const AUDIO_MENU: &[Item] = &[
    item(
        "audio.file",
        "Audio from File…",
        Icon::SpeakerHigh,
        Small,
        Ui(UiAction::Audio),
    ),
    item(
        "audio.library",
        "Media Library…",
        Icon::FolderOpen,
        Small,
        Ui(UiAction::MediaLibrary),
    ),
    item(
        "audio.record",
        "Record Mic…",
        Icon::Headphones,
        Small,
        Ui(UiAction::RecordNarration),
    ),
    item(
        "audio.tts",
        "Text-to-Speech…",
        Icon::SpeakerHigh,
        Small,
        Ui(UiAction::NarrationFromText),
    ),
    item(
        "audio.ai",
        "AI Audio",
        Icon::Sparkle,
        Small,
        NotYet("AI sound effects and music"),
    ),
    item(
        "audio.options",
        "Options…",
        Icon::Gear,
        Small,
        NotYet("the course audio options"),
    ),
];

const INPUT_MENU: &[Item] = &[
    item(
        "input.text",
        "Text Entry",
        Icon::TextT,
        Small,
        Ui(UiAction::InputText),
    ),
    item(
        "input.checkbox",
        "Checkbox",
        Icon::CheckSquare,
        Small,
        Ui(UiAction::InputCheckbox),
    ),
    item(
        "input.radio",
        "Radio Buttons",
        Icon::RadioButton,
        Small,
        Ui(UiAction::InputRadio),
    ),
];

const INSERT: &[Group] = &[
    Group {
        id: "insert.slide",
        label: "Slide",
        launcher: None,
        items: &[
            helped(
                item(
                    "insert.slide_layer",
                    "Slide Layer",
                    Icon::Stack,
                    Large,
                    Ui(UiAction::SlideLayer),
                ),
                "Add a layer to the current slide.",
            ),
            item(
                "insert.convert_question",
                "Convert to Custom Question",
                Icon::Question,
                Large,
                NotYet("turning a slide's objects into a custom question"),
            ),
            item(
                "insert.camera_zoom",
                "Camera Zoom",
                Icon::MagnifyingGlassPlus,
                Large,
                NotYet("camera zoom"),
            ),
        ],
    },
    Group {
        id: "insert.library",
        label: "Library",
        launcher: None,
        items: &[
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "insert.characters",
                "Characters",
                Icon::Robot,
                Large,
                NotPlanned("characters"),
            ),
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "insert.photos",
                "Photos",
                Icon::Image,
                Large,
                NotPlanned("a stock asset library"),
            ),
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "insert.illustrations",
                "Illustrations",
                Icon::Palette,
                Small,
                NotPlanned("a stock asset library"),
            ),
            item(
                "insert.icons",
                "Icons",
                Icon::Shapes,
                Small,
                NotYet("a bundled icon set"),
            ),
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "insert.videos",
                "Videos",
                Icon::FilmStrip,
                Small,
                NotPlanned("a stock asset library"),
            ),
        ],
    },
    Group {
        id: "insert.media",
        label: "Media",
        launcher: None,
        items: &[
            helped(
                split(
                    "insert.picture",
                    "Picture",
                    Icon::Image,
                    LargeSplit,
                    Ui(UiAction::Picture),
                    Menu::Items(PICTURE_MENU),
                ),
                "Insert a picture from a file.",
            ),
            // Kept for possible future use; hidden from the UI by owner decision.
            item(
                "insert.image_360",
                "360° Image",
                Icon::Circle,
                Large,
                NotPlanned("360° images"),
            ),
            helped(
                split(
                    "insert.shape",
                    "Shape",
                    Icon::Shapes,
                    LargeSplit,
                    Ui(UiAction::Shape),
                    Menu::Ui(UiMenu::Shapes),
                ),
                "Pick a shape, then drag on the slide.",
            ),
            helped(
                split(
                    "insert.caption",
                    "Caption",
                    Icon::Subtitles,
                    LargeSplit,
                    Ui(UiAction::Caption),
                    Menu::Ui(UiMenu::Callouts),
                ),
                "Pick a callout shape, then drag on the slide.",
            ),
            split(
                "insert.video",
                "Video",
                Icon::FilmStrip,
                LargeSplit,
                Ui(UiAction::Video),
                Menu::Items(VIDEO_MENU),
            ),
            split(
                "insert.audio",
                "Audio",
                Icon::SpeakerHigh,
                LargeSplit,
                Ui(UiAction::Audio),
                Menu::Items(AUDIO_MENU),
            ),
            helped(
                item(
                    "insert.web_object",
                    "Web Object",
                    Icon::Link,
                    Large,
                    Ui(UiAction::WebObject),
                ),
                "Embed a web page or a video from a website.",
            ),
            helped(
                item(
                    "insert.lottie",
                    "Animation",
                    Icon::Sparkle,
                    Large,
                    Ui(UiAction::Lottie),
                ),
                "Insert a Lottie animation.",
            ),
        ],
    },
    Group {
        id: "insert.text",
        label: "Text",
        launcher: None,
        items: &[
            helped(
                item(
                    "insert.text_box",
                    "Text Box",
                    Icon::TextT,
                    Large,
                    Ui(UiAction::TextBox),
                ),
                "Click on the slide to add a single-line label, or drag to draw a fixed-width text box.",
            ),
            item(
                "insert.table",
                "Table",
                Icon::Rows,
                Large,
                Ui(UiAction::Table),
            ),
            helped(
                split(
                    "insert.slide_number",
                    "Slide Number",
                    Icon::ListNumbers,
                    LargeSplit,
                    Ui(UiAction::InsertSlideNumber),
                    Menu::Items(SLIDE_NUMBER_MENU),
                ),
                "Insert the slide number. It shows the number in the preview and the player.",
            ),
            helped(
                item(
                    "insert.symbol",
                    "Symbol",
                    Icon::TextAa,
                    Small,
                    Ui(UiAction::InsertSymbol),
                ),
                "Insert a symbol such as a copyright sign, an arrow or a Greek letter.",
            ),
            helped(
                item(
                    "insert.reference",
                    "Reference",
                    Icon::BracketsCurly,
                    Small,
                    Ui(UiAction::InsertReference),
                ),
                "Insert a variable reference. It shows the variable's value in the preview and the player.",
            ),
            item(
                "insert.hyperlink",
                "Hyperlink",
                Icon::Link,
                Small,
                NotYet(WIRING_FORMAT),
            ),
        ],
    },
    Group {
        id: "insert.interactive",
        label: "Interactive Objects",
        launcher: None,
        items: &[
            helped(
                item(
                    "insert.button",
                    "Button",
                    Icon::CursorClick,
                    Large,
                    Ui(UiAction::Button),
                ),
                "Drag on the slide to place a button.",
            ),
            item(
                "insert.slider",
                "Slider",
                Icon::SlidersHorizontal,
                Large,
                Ui(UiAction::Slider),
            ),
            item(
                "insert.dial",
                "Dial",
                Icon::Timer,
                Large,
                Ui(UiAction::Dial),
            ),
            item(
                "insert.hotspot",
                "Hotspot",
                Icon::Target,
                Large,
                NotYet("hotspot objects (a hotspot question has its own)"),
            ),
            split(
                "insert.input",
                "Input",
                Icon::TextColumns,
                LargeSplit,
                Ui(UiAction::InputText),
                Menu::Items(INPUT_MENU),
            ),
            item(
                "insert.marker",
                "Marker",
                Icon::Flag,
                Large,
                Ui(UiAction::Marker),
            ),
            helped(
                item(
                    "insert.trigger",
                    "Trigger",
                    Icon::Lightning,
                    Small,
                    Ui(UiAction::Trigger),
                ),
                "Add a trigger to the selected object or the slide.",
            ),
            item(
                "insert.scroll_panel",
                "Scrolling Panel",
                Icon::ArrowsDownUp,
                Small,
                Ui(UiAction::ScrollPanel),
            ),
            item(
                "insert.mouse",
                "Mouse",
                Icon::Cursor,
                Small,
                NotYet("a custom cursor per slide"),
            ),
            helped(
                item(
                    "insert.interaction",
                    "Interaction",
                    Icon::SquaresFour,
                    Large,
                    Ui(UiAction::Interaction),
                ),
                "Tabs, accordions, flip cards and more, built from layers and triggers.",
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Slides
// ---------------------------------------------------------------------------

const IMPORT_MENU: &[Item] = &[
    item(
        "import.pptx",
        "PowerPoint…",
        Icon::MicrosoftPowerpointLogo,
        Small,
        Ui(UiAction::ImportPptx),
    ),
    item(
        "import.questions",
        "Questions from CSV…",
        Icon::ListChecks,
        Small,
        Ui(UiAction::ImportQuestions),
    ),
    item(
        "import.document",
        "Course from Document…",
        Icon::Article,
        Small,
        Ui(UiAction::ImportDocument),
    ),
    item(
        "import.outline",
        "Slides from Outline…",
        Icon::ListBullets,
        Small,
        Ui(UiAction::ImportOutline),
    ),
];

const SLIDES: &[Group] = &[
    Group {
        id: "slides.slide",
        label: "Slide",
        launcher: None,
        items: &[
            Item {
                id: "slides.new_slide",
                ..NEW_SLIDE
            },
            split(
                "slides.import",
                "Import",
                Icon::FilePlus,
                LargeSplit,
                Ui(UiAction::ImportPptx),
                Menu::Items(IMPORT_MENU),
            ),
            item(
                "slides.duplicate",
                "Duplicate",
                Icon::Copy,
                Small,
                Ui(UiAction::DuplicateSlide),
            ),
        ],
    },
    Group {
        id: "slides.scene",
        label: "Scene",
        launcher: None,
        items: &[helped(
            item(
                "slides.new_scene",
                "New Scene",
                Icon::PlusSquare,
                Large,
                Ui(UiAction::NewScene),
            ),
            "Add a scene that starts with one slide.",
        )],
    },
    Group {
        id: "slides.questions",
        label: "Questions",
        launcher: None,
        items: &[
            helped(
                split(
                    "slides.question",
                    "Question",
                    Icon::Question,
                    LargeSplit,
                    Ui(UiAction::Question),
                    Menu::Ui(UiMenu::Questions),
                ),
                "Add a question slide: pick the kind.",
            ),
            item(
                "slides.banks",
                "Question Banks",
                Icon::Cards,
                LargeSplit,
                NotYet(WIRING_BANKS),
            ),
            item(
                "slides.results",
                "Results Slide",
                Icon::Trophy,
                Large,
                Ui(UiAction::ResultsSlide),
            ),
            item(
                "slides.certificate",
                "Certificate",
                Icon::GraduationCap,
                Large,
                Ui(UiAction::Certificate),
            ),
        ],
    },
    Group {
        id: "slides.masters",
        label: "Masters",
        launcher: None,
        items: &[
            item(
                "slides.slide_master",
                "Slide Master",
                Icon::Layout,
                Large,
                Ui(UiAction::SlideMaster),
            ),
            item(
                "slides.feedback_layouts",
                "Feedback Layouts",
                Icon::Layout,
                Large,
                NotYet("feedback layouts"),
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// View
// ---------------------------------------------------------------------------

const VIEW: &[Group] = &[
    Group {
        id: "view.views",
        label: "Views",
        launcher: None,
        items: &[
            helped(
                item(
                    "view.course_map",
                    "Course Map",
                    Icon::SquaresFour,
                    Large,
                    Ui(UiAction::CourseMap),
                ),
                "Every scene and slide, with how they link.",
            ),
            item(
                "view.slide",
                "Slide",
                Icon::Presentation,
                Large,
                Ui(UiAction::SlideView),
            ),
            item(
                "view.slide_master",
                "Slide Master",
                Icon::Layout,
                Large,
                Ui(UiAction::SlideMaster),
            ),
            item(
                "view.feedback_layouts",
                "Feedback Layouts",
                Icon::Layout,
                Large,
                NotYet("feedback layouts"),
            ),
        ],
    },
    Group {
        id: "view.zoom",
        label: "Zoom",
        launcher: None,
        items: &[
            item(
                "view.fit",
                "Fit in Window",
                Icon::CornersIn,
                Large,
                Ui(UiAction::ZoomFit),
            ),
            item(
                "view.zoom_in",
                "Zoom In",
                Icon::MagnifyingGlassPlus,
                Small,
                Ui(UiAction::ZoomIn),
            ),
            item(
                "view.zoom_out",
                "Zoom Out",
                Icon::MagnifyingGlassMinus,
                Small,
                Ui(UiAction::ZoomOut),
            ),
            item(
                "view.zoom_100",
                "100%",
                Icon::MagnifyingGlass,
                Small,
                Ui(UiAction::Zoom100),
            ),
        ],
    },
    Group {
        id: "view.show",
        label: "Show",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::GridGuides,
            Some(crate::panels::Section::Grid),
        ))),
        items: &[
            item(
                "view.rulers",
                "Rulers",
                Icon::Rows,
                Small,
                Ui(UiAction::Rulers),
            ),
            item(
                "view.grid",
                "Grid",
                Icon::SquaresFour,
                Small,
                Ui(UiAction::Grid),
            ),
            item(
                "view.guides",
                "Guides",
                Icon::Plus,
                Small,
                Ui(UiAction::Guides),
            ),
            item(
                "view.snap_grid",
                "Snap to Grid",
                Icon::Selection,
                Small,
                Ui(UiAction::SnapToGrid),
            ),
            item(
                "view.snap_guides",
                "Snap to Guides",
                Icon::SelectionPlus,
                Small,
                Ui(UiAction::SnapToGuides),
            ),
        ],
    },
    Group {
        id: "view.panels",
        label: "Panels",
        launcher: None,
        items: &[
            item(
                "view.panel_slides",
                "Slides",
                Icon::SidebarSimple,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::Slides)),
            ),
            item(
                "view.panel_timeline",
                "Timeline",
                Icon::Clock,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::Timeline)),
            ),
            item(
                "view.panel_format",
                "Format",
                Icon::SlidersHorizontal,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::Format)),
            ),
            item(
                "view.panel_animate",
                "Animate",
                Icon::Sparkle,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::Animate)),
            ),
            item(
                "view.panel_slide",
                "Slide",
                Icon::Presentation,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::SlideInspector)),
            ),
            item(
                "view.panel_course",
                "Course",
                Icon::GraduationCap,
                Small,
                Ui(UiAction::Panel(crate::dock::Panel::CourseInspector)),
            ),
            helped(
                item(
                    "view.panel_text",
                    "Text",
                    Icon::TextT,
                    Small,
                    Ui(UiAction::Panel(crate::dock::Panel::Text)),
                ),
                "Every text setting: font, paragraph, text box. Opens by itself when you edit text.",
            ),
            helped(
                item(
                    "view.reset_panels",
                    "Reset Panels",
                    Icon::ArrowCounterClockwise,
                    Small,
                    Ui(UiAction::ResetPanels),
                ),
                "Put every panel back where it starts.",
            ),
        ],
    },
    Group {
        id: "view.theme",
        label: "Theme",
        launcher: None,
        items: &[
            item(
                "view.light",
                "Light",
                Icon::Sun,
                Small,
                Ui(UiAction::ThemeLight),
            ),
            item(
                "view.dark",
                "Dark",
                Icon::Moon,
                Small,
                Ui(UiAction::ThemeDark),
            ),
            item(
                "view.system",
                "Match System",
                Icon::Monitor,
                Small,
                Ui(UiAction::ThemeSystem),
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Design
// ---------------------------------------------------------------------------

const DESIGN: &[Group] = &[
    Group {
        id: "design.setup",
        label: "Setup",
        launcher: None,
        items: &[helped(
            item(
                "design.slide_size",
                "Slide Size",
                Icon::FrameCorners,
                Large,
                Ui(UiAction::SlideSize),
            ),
            "Change the size of every slide.",
        )],
    },
    Group {
        id: "design.themes",
        label: "Themes",
        launcher: None,
        items: &[helped(
            item(
                "design.themes",
                "Themes",
                Icon::Palette,
                Kind::Gallery { tiles: 5 },
                Ui(UiAction::Gallery(Gallery::Themes)),
            ),
            "Pick a theme: colors for the whole course.",
        )],
    },
    Group {
        id: "design.options",
        label: "Theme Options",
        launcher: None,
        items: &[
            item(
                "design.colors",
                "Colors",
                Icon::Drop,
                Small,
                Ui(UiAction::Themes),
            ),
            split(
                "design.fonts",
                "Fonts",
                Icon::TextAa,
                SmallSplit,
                Ui(UiAction::ThemeFonts),
                Menu::Ui(UiMenu::ThemeFonts),
            ),
        ],
    },
    Group {
        id: "design.background",
        label: "Background",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::SlideInspector,
            None,
        ))),
        items: &[
            item(
                "design.background_styles",
                "Background Styles",
                Icon::Image,
                SmallSplit,
                NotYet(WIRING_SLIDE),
            ),
            item(
                "design.hide_background",
                "Hide Background Graphics",
                Icon::EyeSlash,
                Small,
                NotYet("hiding the master's graphics on one slide"),
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Transitions
// ---------------------------------------------------------------------------

const TRANSITIONS: &[Group] = &[
    Group {
        id: "transitions.to_this_slide",
        label: "Transition to This Slide",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::SlideInspector,
            None,
        ))),
        items: &[
            helped(
                item(
                    "transitions.pick",
                    "Transitions",
                    Icon::FilmStrip,
                    Kind::Gallery { tiles: 4 },
                    Ui(UiAction::Gallery(Gallery::Transitions)),
                ),
                "How this slide comes in.",
            ),
            split(
                "transitions.options",
                "Effect Options",
                Icon::SlidersHorizontal,
                LargeSplit,
                Ui(UiAction::Effect(EffectOp::Options(Motion::Transition))),
                Menu::Ui(UiMenu::EffectOptions(Motion::Transition)),
            ),
        ],
    },
    Group {
        id: "transitions.timing",
        label: "Timing",
        launcher: None,
        items: &[
            item(
                "transitions.duration",
                "Duration",
                Icon::Timer,
                Kind::Number { width: 64.0 },
                Ui(UiAction::Effect(EffectOp::Duration(Motion::Transition))),
            ),
            helped(
                item(
                    "transitions.apply_all",
                    "Apply To All",
                    Icon::Stack,
                    Small,
                    Ui(UiAction::Effect(EffectOp::TransitionToAll)),
                ),
                "Give every slide this slide's transition.",
            ),
        ],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Animations
// ---------------------------------------------------------------------------

const ANIMATIONS: &[Group] = &[
    Group {
        id: "animations.advanced",
        label: "Advanced",
        launcher: None,
        items: &[item(
            "animations.painter",
            "Animation Painter",
            Icon::PaintBrush,
            Large,
            Ui(UiAction::AnimationPainter),
        )],
    },
    Group {
        id: "animations.entrance",
        label: "Entrance Animations",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Animate, None))),
        items: &[
            helped(
                item(
                    "animations.entrance",
                    "Entrance",
                    Icon::ArrowRight,
                    Kind::Gallery { tiles: 4 },
                    Ui(UiAction::Gallery(Gallery::Entrance)),
                ),
                "How the selected objects come in.",
            ),
            split(
                "animations.entrance_options",
                "Effect Options",
                Icon::SlidersHorizontal,
                SmallSplit,
                Ui(UiAction::Effect(EffectOp::Options(Motion::Entrance))),
                Menu::Ui(UiMenu::EffectOptions(Motion::Entrance)),
            ),
            item(
                "animations.entrance_duration",
                "Duration",
                Icon::Timer,
                Kind::Number { width: 56.0 },
                Ui(UiAction::Effect(EffectOp::Duration(Motion::Entrance))),
            ),
        ],
    },
    Group {
        id: "animations.exit",
        label: "Exit Animations",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Animate, None))),
        items: &[
            helped(
                item(
                    "animations.exit",
                    "Exit",
                    Icon::ArrowLeft,
                    Kind::Gallery { tiles: 4 },
                    Ui(UiAction::Gallery(Gallery::Exit)),
                ),
                "How the selected objects go out.",
            ),
            split(
                "animations.exit_options",
                "Effect Options",
                Icon::SlidersHorizontal,
                SmallSplit,
                Ui(UiAction::Effect(EffectOp::Options(Motion::Exit))),
                Menu::Ui(UiMenu::EffectOptions(Motion::Exit)),
            ),
            item(
                "animations.exit_duration",
                "Duration",
                Icon::Timer,
                Kind::Number { width: 56.0 },
                Ui(UiAction::Effect(EffectOp::Duration(Motion::Exit))),
            ),
        ],
    },
    Group {
        id: "animations.emphasis",
        label: "Emphasis Animations",
        launcher: None,
        items: &[helped(
            item(
                "animations.emphasis",
                "Add Emphasis Animation",
                Icon::Sparkle,
                Kind::Gallery { tiles: 4 },
                Ui(UiAction::Gallery(Gallery::Emphasis)),
            ),
            "Add an emphasis to the selected objects, at the playhead.",
        )],
    },
    Group {
        id: "animations.motion",
        label: "Motion Paths",
        launcher: None,
        items: &[helped(
            item(
                "animations.motion_path",
                "Add Motion Path",
                Icon::Path,
                Large,
                Ui(UiAction::AddMotionPath),
            ),
            "Move the selected object along a path.",
        )],
    },
    Group {
        id: "animations.size",
        label: "Size and Position",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::SizePosition,
            Some(crate::panels::Section::Size),
        ))),
        items: &[item(
            "animations.size_position",
            "Size and Position",
            Icon::CornersOut,
            Large,
            Ui(UiAction::PanelAt(
                crate::dock::Panel::SizePosition,
                Some(crate::panels::Section::Size),
            )),
        )],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Help
// ---------------------------------------------------------------------------

const AI: &[Group] = &[
    Group {
        id: "ai.write",
        label: "Write",
        launcher: None,
        items: &[
            item(
                "ai.insert_text",
                "Insert Text",
                Icon::Sparkle,
                Large,
                NotYet("AI text drafting"),
            ),
            item(
                "ai.edit_text",
                "Edit Text",
                Icon::PencilSimple,
                Large,
                NotYet("AI rewriting of selected text"),
            ),
            item(
                "ai.summary",
                "Summary",
                Icon::Article,
                Large,
                NotYet("AI summaries"),
            ),
            helped(
                item(
                    "ai.alt_text",
                    "Suggest Alt Text",
                    Icon::Eye,
                    Large,
                    Ui(UiAction::AltText),
                ),
                "Describe the course's pictures for screen readers.",
            ),
        ],
    },
    Group {
        id: "ai.media",
        label: "Media",
        launcher: None,
        items: &[
            item(
                "ai.insert_image",
                "Insert Image",
                Icon::Image,
                Large,
                NotYet("AI images"),
            ),
            split(
                "ai.insert_audio",
                "Insert Audio",
                Icon::SpeakerHigh,
                LargeSplit,
                Ui(UiAction::NarrationFromText),
                Menu::Items(&[
                    item(
                        "ai.audio.narration",
                        "Narration from Text…",
                        Icon::SpeakerHigh,
                        Small,
                        Ui(UiAction::NarrationFromText),
                    ),
                    item(
                        "ai.audio.effect",
                        "Sound Effect",
                        Icon::SpeakerHigh,
                        Small,
                        NotYet("AI sound effects"),
                    ),
                    item(
                        "ai.audio.music",
                        "Music",
                        Icon::SpeakerHigh,
                        Small,
                        NotYet("AI music"),
                    ),
                ]),
            ),
        ],
    },
    Group {
        id: "ai.assess",
        label: "Assess",
        launcher: None,
        items: &[
            item(
                "ai.quiz",
                "Quiz",
                Icon::ListChecks,
                Large,
                NotYet("AI quiz drafting"),
            ),
            item(
                "ai.question",
                "Question",
                Icon::Question,
                Large,
                NotYet("AI question drafting"),
            ),
        ],
    },
    Group {
        id: "ai.agents",
        label: "Agents",
        launcher: None,
        items: &[
            item(
                "ai.connect_agent",
                "Connect an AI Agent…",
                Icon::Robot,
                Small,
                Ui(UiAction::ConnectAgent),
            ),
            item(
                "ai.settings",
                "AI Settings…",
                Icon::Gear,
                Small,
                Ui(UiAction::AiSettings),
            ),
            item(
                "ai.automation",
                "Allow Automation",
                Icon::Lock,
                Small,
                Ui(UiAction::Automation),
            ),
        ],
    },
];

const HELP: &[Group] = &[
    Group {
        id: "help.help",
        label: "Help",
        launcher: None,
        items: &[
            item(
                "help.guide",
                "Guide",
                Icon::BookOpen,
                Large,
                Ui(UiAction::Guide),
            ),
            item(
                "help.shortcuts",
                "Keyboard Shortcuts",
                Icon::Keyboard,
                Large,
                Ui(UiAction::Shortcuts),
            ),
            item(
                "help.welcome",
                "Welcome Screen",
                Icon::House,
                Large,
                Ui(UiAction::Welcome),
            ),
        ],
    },
    Group {
        id: "help.community",
        label: "Community",
        launcher: None,
        items: &[
            item(
                "help.github",
                "CourseCraft on GitHub",
                Icon::ArrowSquareOut,
                Small,
                Ui(UiAction::GitHub),
            ),
            item(
                "help.report",
                "Report a Problem…",
                Icon::Warning,
                Small,
                Ui(UiAction::ReportProblem),
            ),
            item(
                "help.suggest",
                "Suggest a Feature…",
                Icon::Lightning,
                Small,
                Ui(UiAction::SuggestFeature),
            ),
        ],
    },
    Group {
        id: "help.updates",
        label: "Updates",
        launcher: None,
        items: &[item(
            "help.check_updates",
            "Check for Updates",
            Icon::ArrowClockwise,
            Large,
            Ui(UiAction::CheckUpdates),
        )],
    },
    Group {
        id: "help.about",
        label: "About",
        launcher: None,
        items: &[item(
            "help.about",
            "About CourseCraft",
            Icon::Info,
            Large,
            NotYet("an About window"),
        )],
    },
];

// ---------------------------------------------------------------------------
// Contextual tabs (contextual-tabs.md)
// ---------------------------------------------------------------------------

/// Why a contextual control whose feature exists is still in the Format panel.
const WIRING_MEDIA: &str = "ribbon wiring (use the Format inspector's media rows)";
const WIRING_QUESTION: &str = "ribbon wiring (use the Question panel)";

const fn ctx(op: ContextOp) -> Action {
    Ui(UiAction::Context(op))
}

const fn number(id: &'static str, label: &'static str, field: FrameField) -> Item {
    item(
        id,
        label,
        Icon::FrameCorners,
        Kind::Number { width: 64.0 },
        ctx(ContextOp::Frame(field)),
    )
}

const ARRANGE_AND_ALIGN: Group = Group {
    id: "ctx_arrange",
    label: "Arrange and Align",
    launcher: None,
    items: &[
        helped(
            split(
                "ctx.arrange",
                "Arrange",
                Icon::Stack,
                LargeSplit,
                Ui(UiAction::Arrange),
                Menu::Ui(UiMenu::Arrange),
            ),
            "Order, align and distribute the selected objects.",
        ),
        item(
            "ctx.group",
            "Group",
            Icon::SquaresFour,
            Small,
            Command("object.group"),
        ),
        item(
            "ctx.ungroup",
            "Ungroup",
            Icon::SquaresFour,
            Small,
            Command("object.ungroup"),
        ),
        helped(
            item(
                "ctx.group_button",
                "Make button from group",
                Icon::CursorClick,
                Small,
                Command("group.set_interactive"),
            ),
            "Share button states and triggers across every object in the selected group.",
        ),
        item(
            "ctx.rotate",
            "Rotate",
            Icon::ArrowClockwise,
            Small,
            Ui(UiAction::PanelAt(
                crate::dock::Panel::SizePosition,
                Some(crate::panels::Section::Rotate),
            )),
        ),
    ],
};

const SIZE_AND_POSITION: Group = Group {
    id: "ctx_size",
    label: "Size and Position",
    launcher: Some(Ui(UiAction::PanelAt(
        crate::dock::Panel::SizePosition,
        Some(crate::panels::Section::Size),
    ))),
    items: &[
        number("ctx.x", "X", FrameField::X),
        number("ctx.y", "Y", FrameField::Y),
        number("ctx.width", "Width", FrameField::Width),
        number("ctx.height", "Height", FrameField::Height),
    ],
};

const SHAPE_STYLE_LAUNCHER: Option<Action> = Some(Ui(UiAction::PanelAt(
    crate::dock::Panel::ShapeStyle,
    Some(crate::panels::Section::Fill),
)));

const SHAPE_FILL: Action = Ui(UiAction::PanelAt(
    crate::dock::Panel::ShapeStyle,
    Some(crate::panels::Section::Fill),
));
const SHAPE_OUTLINE: Action = Ui(UiAction::PanelAt(
    crate::dock::Panel::ShapeStyle,
    Some(crate::panels::Section::Outline),
));
const SHAPE_EFFECT: Action = Ui(UiAction::PanelAt(crate::dock::Panel::ShapeStyle, None));

const DRAWING_FORMAT: &[Group] = &[
    Group {
        id: "insert_shapes",
        label: "Insert Shapes",
        launcher: None,
        items: &[
            helped(
                split(
                    "drawing_fmt.shapes",
                    "Shapes",
                    Icon::Shapes,
                    LargeSplit,
                    Ui(UiAction::Shape),
                    Menu::Ui(UiMenu::Shapes),
                ),
                "Draw a shape on the slide.",
            ),
            item(
                "drawing_fmt.change_shape",
                "Change Shape",
                Icon::Shapes,
                SmallSplit,
                NotYet("changing a shape's geometry and keeping its format"),
            ),
            helped(
                item(
                    "drawing_fmt.edit_points",
                    "Edit Points",
                    Icon::Path,
                    Small,
                    ctx(ContextOp::EditPoints),
                ),
                "Move the points of the selected freeform.",
            ),
            item(
                "drawing_fmt.text_box",
                "Text Box",
                Icon::TextT,
                Small,
                Ui(UiAction::TextBox),
            ),
        ],
    },
    Group {
        id: "shape_styles",
        label: "Shape Styles",
        launcher: SHAPE_STYLE_LAUNCHER,
        items: &[
            item(
                "drawing_fmt.styles",
                "Quick Styles",
                Icon::Palette,
                LargeSplit,
                NotYet("the style gallery in the ribbon"),
            ),
            item(
                "drawing_fmt.fill",
                "Shape Fill",
                Icon::PaintBrush,
                Small,
                SHAPE_FILL,
            ),
            item(
                "drawing_fmt.outline",
                "Shape Outline",
                Icon::Square,
                Small,
                SHAPE_OUTLINE,
            ),
            item(
                "drawing_fmt.effect",
                "Shape Effect",
                Icon::Sparkle,
                Small,
                SHAPE_EFFECT,
            ),
        ],
    },
    ARRANGE_AND_ALIGN,
    SIZE_AND_POSITION,
    PUBLISH,
];

const BUTTON_FORMAT: &[Group] = &[
    Group {
        id: "button_action",
        label: "Action",
        launcher: None,
        items: &[helped(
            item(
                "button_fmt.action",
                "On Click",
                Icon::CursorClick,
                Large,
                Ui(UiAction::Trigger),
            ),
            "Add a trigger that runs when this button is clicked.",
        )],
    },
    Group {
        id: "button_icons",
        label: "Button Icons",
        launcher: None,
        items: &[
            item(
                "button_fmt.icons",
                "Icons",
                Icon::Sparkle,
                LargeSplit,
                NotYet("icons inside buttons"),
            ),
            item(
                "button_fmt.icon_align",
                "Align Icon",
                Icon::TextAlignLeft,
                SmallSplit,
                NotYet("icons inside buttons"),
            ),
            item(
                "button_fmt.icon_color",
                "Icon Color",
                Icon::Palette,
                SmallSplit,
                NotYet("icons inside buttons"),
            ),
            item(
                "button_fmt.icon_remove",
                "Remove Icon",
                Icon::X,
                Small,
                NotYet("icons inside buttons"),
            ),
        ],
    },
    Group {
        id: "button_styles",
        label: "Button Styles",
        // A button's label and fill live in the Format inspector: Shape Style only styles
        // shapes (`N94`).
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "button_fmt.styles",
                "Quick Styles",
                Icon::Palette,
                LargeSplit,
                NotYet("the style gallery in the ribbon"),
            ),
            item(
                "button_fmt.fill",
                "Button Fill",
                Icon::PaintBrush,
                Small,
                Ui(UiAction::FormatInspector),
            ),
            item(
                "button_fmt.border",
                "Button Border",
                Icon::Square,
                Small,
                NotYet("button borders"),
            ),
            item(
                "button_fmt.effects",
                "Button Effects",
                Icon::Sparkle,
                Small,
                SHAPE_EFFECT,
            ),
        ],
    },
    ARRANGE_AND_ALIGN,
    SIZE_AND_POSITION,
    PUBLISH,
];

const PICTURE_FORMAT: &[Group] = &[
    Group {
        id: "picture_adjust",
        label: "Adjust",
        launcher: None,
        items: &[
            item(
                "picture_fmt.brightness",
                "Brightness",
                Icon::Sun,
                SmallSplit,
                NotYet("picture brightness"),
            ),
            item(
                "picture_fmt.contrast",
                "Contrast",
                Icon::Drop,
                SmallSplit,
                NotYet("picture contrast"),
            ),
            item(
                "picture_fmt.recolor",
                "Recolor",
                Icon::Palette,
                SmallSplit,
                NotYet("recolouring photos"),
            ),
            helped(
                item(
                    "picture_fmt.replace",
                    "Replace Picture",
                    Icon::Image,
                    Small,
                    ctx(ContextOp::ReplaceMedia),
                ),
                "Pick another picture file; size, position and triggers stay.",
            ),
            helped(
                item(
                    "picture_fmt.reset",
                    "Reset Picture",
                    Icon::ArrowCounterClockwise,
                    Small,
                    ctx(ContextOp::ResetPicture),
                ),
                "Remove the crop and go back to the picture's own size.",
            ),
        ],
    },
    Group {
        id: "picture_styles",
        label: "Picture Styles",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "picture_fmt.styles",
                "Picture Styles",
                Icon::FrameCorners,
                LargeSplit,
                NotYet("picture style presets"),
            ),
            item(
                "picture_fmt.shape",
                "Picture Shape",
                Icon::Circle,
                SmallSplit,
                NotYet("cropping a picture to a shape"),
            ),
            item(
                "picture_fmt.border",
                "Picture Border",
                Icon::Square,
                SmallSplit,
                NotYet(WIRING_FORMAT),
            ),
            item(
                "picture_fmt.effects",
                "Picture Effects",
                Icon::Sparkle,
                SmallSplit,
                NotYet(WIRING_FORMAT),
            ),
        ],
    },
    ARRANGE_AND_ALIGN,
    Group {
        id: "picture_size",
        label: "Size",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::SizePosition,
            Some(crate::panels::Section::Size),
        ))),
        items: &[
            item(
                "picture_fmt.zoom",
                "Zoom Picture",
                Icon::MagnifyingGlassPlus,
                Large,
                NotYet("enlarging a picture when the learner clicks it"),
            ),
            helped(
                item(
                    "picture_fmt.crop",
                    "Crop",
                    Icon::FrameCorners,
                    Large,
                    ctx(ContextOp::Crop),
                ),
                "Drag the crop handles on the canvas; Enter applies.",
            ),
            number("picture_fmt.x", "X", FrameField::X),
            number("picture_fmt.y", "Y", FrameField::Y),
            number("picture_fmt.width", "Width", FrameField::Width),
            number("picture_fmt.height", "Height", FrameField::Height),
        ],
    },
    PUBLISH,
];

const VIDEO_OPTIONS: &[Group] = &[
    Group {
        id: "video_play",
        label: "Play",
        launcher: None,
        items: &[item(
            "video_opt.play",
            "Play",
            Icon::Play,
            Large,
            NotYet("playing one media object in the editor"),
        )],
    },
    Group {
        id: "video_options",
        label: "Video Options",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "video_opt.volume",
                "Volume",
                Icon::SpeakerHigh,
                LargeSplit,
                NotYet(WIRING_MEDIA),
            ),
            item(
                "video_opt.editor",
                "Video Editor",
                Icon::FilmStrip,
                Large,
                NotYet(WIRING_MEDIA),
            ),
            helped(
                item(
                    "video_opt.replace",
                    "Replace Video",
                    Icon::VideoCamera,
                    Small,
                    ctx(ContextOp::ReplaceMedia),
                ),
                "Pick another video file; size, position and triggers stay.",
            ),
            item(
                "video_opt.export",
                "Export Video",
                Icon::Export,
                Small,
                NotYet("exporting media from the course"),
            ),
            item(
                "video_opt.show",
                "Show Video",
                Icon::Monitor,
                SmallSplit,
                NotYet("playing video in a new window"),
            ),
            item(
                "video_opt.play_mode",
                "Play Video",
                Icon::PlayCircle,
                SmallSplit,
                NotYet(WIRING_MEDIA),
            ),
            item(
                "video_opt.controls",
                "Video Controls",
                Icon::SlidersHorizontal,
                SmallSplit,
                NotYet(WIRING_MEDIA),
            ),
            item(
                "video_opt.compression",
                "Compression",
                Icon::ArrowsIn,
                SmallSplit,
                NotYet("media compression settings"),
            ),
        ],
    },
    Group {
        id: "video_captions",
        label: "Closed Captions",
        launcher: None,
        items: &[helped(
            item(
                "video_opt.captions",
                "Import Captions",
                Icon::Subtitles,
                Large,
                ctx(ContextOp::ImportCaptions),
            ),
            "Add captions from a WebVTT or SRT file.",
        )],
    },
    PUBLISH,
];

const AUDIO_OPTIONS: &[Group] = &[
    Group {
        id: "audio_play",
        label: "Play",
        launcher: None,
        items: &[item(
            "audio_opt.play",
            "Play",
            Icon::Play,
            Large,
            NotYet("playing one media object in the editor"),
        )],
    },
    Group {
        id: "audio_options",
        label: "Audio Options",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "audio_opt.volume",
                "Audio Volume",
                Icon::SpeakerHigh,
                LargeSplit,
                NotYet(WIRING_MEDIA),
            ),
            item(
                "audio_opt.editor",
                "Audio Editor",
                Icon::Headphones,
                Large,
                NotYet(WIRING_MEDIA),
            ),
            helped(
                item(
                    "audio_opt.replace",
                    "Replace Audio",
                    Icon::SpeakerHigh,
                    Small,
                    ctx(ContextOp::ReplaceMedia),
                ),
                "Pick another audio file; timing and triggers stay.",
            ),
            item(
                "audio_opt.export",
                "Export Audio",
                Icon::Export,
                Small,
                NotYet("exporting media from the course"),
            ),
            item(
                "audio_opt.compression",
                "Compression",
                Icon::ArrowsIn,
                SmallSplit,
                NotYet("media compression settings"),
            ),
        ],
    },
    Group {
        id: "audio_captions",
        label: "Closed Captions",
        launcher: None,
        items: &[helped(
            item(
                "audio_opt.captions",
                "Import Captions",
                Icon::Subtitles,
                Large,
                ctx(ContextOp::ImportCaptions),
            ),
            "Add captions from a WebVTT or SRT file.",
        )],
    },
    PUBLISH,
];

const TABLE_DESIGN: &[Group] = &[
    Group {
        id: "table_style_options",
        label: "Table Style Options",
        launcher: None,
        items: &[helped(
            item(
                "table_design.header_row",
                "Header Row",
                Icon::Rows,
                Small,
                ctx(ContextOp::TableHeaderRow),
            ),
            "Format the first row as a header.",
        )],
    },
    Group {
        id: "table_styles",
        label: "Table Styles",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "table_design.styles",
                "Table Styles",
                Icon::SquaresFour,
                LargeSplit,
                NotYet("table style presets"),
            ),
            item(
                "table_design.shading",
                "Shading",
                Icon::PaintBrush,
                SmallSplit,
                NotYet(WIRING_FORMAT),
            ),
            item(
                "table_design.borders",
                "Borders",
                Icon::Square,
                SmallSplit,
                NotYet(WIRING_FORMAT),
            ),
        ],
    },
    PUBLISH,
];

const TABLE_LAYOUT: &[Group] = &[
    Group {
        id: "table_rows_columns",
        label: "Rows & Columns",
        launcher: None,
        items: &[
            item(
                "table_layout.delete_row",
                "Delete Row",
                Icon::Trash,
                Small,
                ctx(ContextOp::TableDeleteRow),
            ),
            item(
                "table_layout.delete_column",
                "Delete Column",
                Icon::Trash,
                Small,
                ctx(ContextOp::TableDeleteColumn),
            ),
            item(
                "table_layout.insert_above",
                "Insert Above",
                Icon::ArrowUp,
                Small,
                ctx(ContextOp::TableInsertAbove),
            ),
            item(
                "table_layout.insert_below",
                "Insert Below",
                Icon::ArrowDown,
                Small,
                ctx(ContextOp::TableInsertBelow),
            ),
            item(
                "table_layout.insert_left",
                "Insert Left",
                Icon::ArrowLeft,
                Small,
                ctx(ContextOp::TableInsertLeft),
            ),
            item(
                "table_layout.insert_right",
                "Insert Right",
                Icon::ArrowRight,
                Small,
                ctx(ContextOp::TableInsertRight),
            ),
        ],
    },
    Group {
        id: "table_merge",
        label: "Merge",
        launcher: None,
        items: &[
            item(
                "table_layout.merge",
                "Merge Cells",
                Icon::CornersIn,
                Small,
                NotYet("selecting a range of table cells"),
            ),
            helped(
                item(
                    "table_layout.split",
                    "Split Cells",
                    Icon::CornersOut,
                    Small,
                    ctx(ContextOp::TableSplit),
                ),
                "Split the selected merged cell back into single cells.",
            ),
        ],
    },
    Group {
        id: "table_cell_size",
        label: "Cell Size",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Format, None))),
        items: &[
            item(
                "table_layout.row_height",
                "Row Height",
                Icon::ArrowsDownUp,
                Small,
                NotYet(WIRING_FORMAT),
            ),
            item(
                "table_layout.column_width",
                "Column Width",
                Icon::ArrowsOut,
                Small,
                NotYet(WIRING_FORMAT),
            ),
        ],
    },
    Group {
        id: "table_alignment",
        label: "Alignment",
        launcher: None,
        items: &[item(
            "table_layout.align",
            "Cell Alignment",
            Icon::TextAlignCenter,
            SmallSplit,
            NotYet(WIRING_FORMAT),
        )],
    },
    PUBLISH,
];

const QUESTION_DESIGN: &[Group] = &[
    Group {
        id: "question_feedback",
        label: "Feedback",
        launcher: None,
        items: &[item(
            "question_design.feedback_layouts",
            "Feedback Layouts",
            Icon::Layout,
            LargeSplit,
            NotYet("feedback layouts"),
        )],
    },
    Group {
        id: "question_display",
        label: "Display",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Question, None))),
        items: &[
            item(
                "question_design.feedback",
                "Feedback",
                Icon::Notepad,
                SmallSplit,
                NotYet(WIRING_QUESTION),
            ),
            helped(
                item(
                    "question_design.shuffle",
                    "Shuffle Answers",
                    Icon::ArrowsDownUp,
                    Small,
                    ctx(ContextOp::QuestionShuffle),
                ),
                "Show the choices in a new order each time.",
            ),
            item(
                "question_design.results",
                "Results Slide",
                Icon::Trophy,
                SmallSplit,
                NotYet(WIRING_QUESTION),
            ),
        ],
    },
    Group {
        id: "question_scoring",
        label: "Scoring",
        launcher: Some(Ui(UiAction::PanelAt(crate::dock::Panel::Question, None))),
        items: &[
            item(
                "question_design.score",
                "Score",
                Icon::Target,
                SmallSplit,
                NotYet(WIRING_QUESTION),
            ),
            item(
                "question_design.attempts",
                "Attempts",
                Icon::ArrowClockwise,
                SmallSplit,
                NotYet(WIRING_QUESTION),
            ),
        ],
    },
    Group {
        id: "question_edit",
        label: "Question",
        launcher: None,
        items: &[helped(
            item(
                "question_design.edit",
                "Edit Question",
                Icon::PencilSimple,
                Large,
                ctx(ContextOp::QuestionPanel),
            ),
            "Open the question's choices, feedback and scoring.",
        )],
    },
    PUBLISH,
];

// ---------------------------------------------------------------------------
// Slide Master (contextual-tabs.md › Slide Master tab)
// ---------------------------------------------------------------------------

const SLIDE_MASTER: &[Group] = &[
    Group {
        id: "master.edit",
        label: "Edit Master",
        launcher: None,
        items: &[
            helped(
                item(
                    "master.insert",
                    "Insert Slide Master",
                    Icon::PlusSquare,
                    Large,
                    Ui(UiAction::InsertMaster),
                ),
                "Add a new, empty slide master.",
            ),
            item(
                "master.insert_layout",
                "Insert Layout",
                Icon::Layout,
                Large,
                NotYet("layouts inside a slide master"),
            ),
            item(
                "master.delete",
                "Delete",
                Icon::Trash,
                Small,
                Ui(UiAction::DeleteMaster),
            ),
            item(
                "master.rename",
                "Rename",
                Icon::PencilSimple,
                Small,
                Ui(UiAction::RenameMaster),
            ),
            item(
                "master.preserve",
                "Preserve",
                Icon::Lock,
                Small,
                NotYet("keeping an unused master"),
            ),
        ],
    },
    Group {
        id: "master.layout",
        label: "Master Layout",
        launcher: None,
        items: &[
            item(
                "master.placeholder",
                "Insert Placeholder",
                Icon::FrameCorners,
                LargeSplit,
                NotYet("placeholders on a master"),
            ),
            item(
                "master.title",
                "Title",
                Icon::TextT,
                Small,
                NotYet("placeholders on a master"),
            ),
        ],
    },
    Group {
        id: "master.theme",
        label: "Edit Theme",
        launcher: None,
        items: &[
            item(
                "master.themes",
                "Themes",
                Icon::Palette,
                Kind::Gallery { tiles: 3 },
                Ui(UiAction::Gallery(Gallery::Themes)),
            ),
            item(
                "master.colors",
                "Colors",
                Icon::Drop,
                Small,
                Ui(UiAction::Themes),
            ),
            item(
                "master.fonts",
                "Fonts",
                Icon::TextAa,
                Small,
                Ui(UiAction::ThemeFonts),
            ),
        ],
    },
    Group {
        id: "master.background",
        label: "Background",
        launcher: Some(Ui(UiAction::PanelAt(
            crate::dock::Panel::SlideInspector,
            None,
        ))),
        items: &[
            item(
                "master.background_styles",
                "Background Styles",
                Icon::Image,
                SmallSplit,
                NotYet(WIRING_SLIDE),
            ),
            item(
                "master.hide_background",
                "Hide Background Graphics",
                Icon::EyeSlash,
                Small,
                NotYet("hiding the master's graphics on one slide"),
            ),
        ],
    },
    Group {
        id: "master.close",
        label: "Close",
        launcher: None,
        items: &[helped(
            item(
                "master.close_view",
                "Close Master View",
                Icon::XCircle,
                Large,
                Ui(UiAction::SlideView),
            ),
            "Go back to the slides.",
        )],
    },
];

/// The Slide Master tab, first in the strip while Master View is open.
pub const MASTER_TAB: Tab = Tab {
    id: "slide_master",
    label: "Slide Master",
    when: TabWhen::Always,
    groups: SLIDE_MASTER,
    map_groups: None,
};

/// The main tabs drawn next to Slide Master in Master View, in order.
pub const MASTER_STRIP: &[&str] = &[
    "home",
    "insert",
    "transitions",
    "animations",
    "view",
    "help",
];

/// A tab shown right of Help, under its context's caption, while the
/// selection needs it.
pub struct ContextTab {
    pub context: Context,
    pub tab: Tab,
}

const fn context_tab(
    context: Context,
    id: &'static str,
    label: &'static str,
    groups: &'static [Group],
) -> ContextTab {
    ContextTab {
        context,
        tab: Tab {
            id,
            label,
            when: TabWhen::SlideOpen,
            groups,
            map_groups: None,
        },
    }
}

/// The contextual tabs, in strip order (contextual-tabs.md).
pub const CONTEXT_TABS: &[ContextTab] = &[
    context_tab(Context::Drawing, "drawing_format", "Format", DRAWING_FORMAT),
    context_tab(Context::Button, "button_format", "Format", BUTTON_FORMAT),
    context_tab(Context::Picture, "picture_format", "Format", PICTURE_FORMAT),
    context_tab(Context::Video, "video_options", "Options", VIDEO_OPTIONS),
    context_tab(Context::Audio, "audio_options", "Options", AUDIO_OPTIONS),
    context_tab(Context::Table, "table_design", "Design", TABLE_DESIGN),
    context_tab(Context::Table, "table_layout", "Layout", TABLE_LAYOUT),
    context_tab(
        Context::Question,
        "question_design",
        "Design",
        QUESTION_DESIGN,
    ),
];

// ---------------------------------------------------------------------------
// The strip
// ---------------------------------------------------------------------------

/// The tab strip after File, in order (ribbon.md › Tab strip).
pub const TABS: &[Tab] = &[
    Tab {
        id: "home",
        label: "Home",
        when: TabWhen::Always,
        groups: HOME_SLIDE,
        map_groups: Some(HOME_MAP),
    },
    Tab {
        id: "insert",
        label: "Insert",
        when: TabWhen::SlideOpen,
        groups: INSERT,
        map_groups: None,
    },
    Tab {
        id: "slides",
        label: "Slides",
        when: TabWhen::Always,
        groups: SLIDES,
        map_groups: None,
    },
    Tab {
        id: "design",
        label: "Design",
        when: TabWhen::Always,
        groups: DESIGN,
        map_groups: None,
    },
    Tab {
        id: "transitions",
        label: "Transitions",
        when: TabWhen::SlideOpen,
        groups: TRANSITIONS,
        map_groups: None,
    },
    Tab {
        id: "animations",
        label: "Animations",
        when: TabWhen::SlideOpen,
        groups: ANIMATIONS,
        map_groups: None,
    },
    Tab {
        id: "view",
        label: "View",
        when: TabWhen::Always,
        groups: VIEW,
        map_groups: None,
    },
    Tab {
        id: "ai",
        label: "AI",
        when: TabWhen::Always,
        groups: AI,
        map_groups: None,
    },
    Tab {
        id: "help",
        label: "Help",
        when: TabWhen::Always,
        groups: HELP,
        map_groups: None,
    },
];

/// The tab with this id.
#[must_use]
pub fn tab(id: &str) -> Option<&'static Tab> {
    TABS.iter()
        .chain(std::iter::once(&MASTER_TAB))
        .chain(CONTEXT_TABS.iter().map(|c| &c.tab))
        .find(|t| t.id == id)
}

/// The contextual tab with this id.
#[must_use]
pub fn context_tab_of(id: &str) -> Option<&'static ContextTab> {
    CONTEXT_TABS.iter().find(|c| c.tab.id == id)
}

/// Every item of every tab, menus included, for checks.
pub fn all_items() -> impl Iterator<Item = &'static Item> {
    fn with_menu(item: &'static Item) -> Vec<&'static Item> {
        let mut out = vec![item];
        if let Menu::Items(entries) = item.menu {
            out.extend(entries.iter());
        }
        out
    }
    TABS.iter()
        .chain(std::iter::once(&MASTER_TAB))
        .chain(CONTEXT_TABS.iter().map(|c| &c.tab))
        .flat_map(|t| t.groups.iter().chain(t.map_groups.unwrap_or(&[]).iter()))
        .flat_map(|g| g.items.iter())
        .flat_map(with_menu)
}
