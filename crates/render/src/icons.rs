//! Phosphor icon helpers, codepoints, and variants.
//!
//! Provides the icon set defined in `crates/render/fonts/phosphor-icons.txt` and
//! `phosphor-codepoints.txt`. Sizes follow tokens: [`ICON_SM`] (16 px) for compact
//! controls and lists, and [`ICON_TOOLBAR`] (18 px) for toolbar buttons.

use egui::{FontFamily, RichText};

/// Compact icon size for inspector rows, lists, and status bar (16 px).
pub const ICON_SM: f32 = 16.0;

/// Toolbar icon size (18 px).
pub const ICON_TOOLBAR: f32 = 18.0;

/// Phosphor icon variants bundled in `Phosphor.ttf` and `Phosphor-Fill.ttf`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Icon {
    ArrowClockwise,
    ArrowCounterClockwise,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowSquareOut,
    ArrowUp,
    ArrowsDownUp,
    ArrowsIn,
    ArrowsOut,
    Article,
    BookOpen,
    BracketsCurly,
    CaretDown,
    CaretLeft,
    CaretRight,
    CaretUp,
    Cards,
    Check,
    CheckCircle,
    CheckSquare,
    Circle,
    Clock,
    Copy,
    CornersIn,
    CornersOut,
    Cursor,
    CursorClick,
    DotsThree,
    DotsThreeVertical,
    Drop,
    Export,
    Eye,
    EyeSlash,
    FilePlus,
    FilmStrip,
    Flag,
    FloppyDisk,
    FolderOpen,
    FrameCorners,
    Gear,
    GraduationCap,
    HandGrabbing,
    HandPointing,
    Headphones,
    House,
    Image,
    Info,
    Keyboard,
    Layout,
    Lightning,
    Link,
    List,
    ListBullets,
    ListChecks,
    ListNumbers,
    Lock,
    LockOpen,
    LockSimple,
    MagicWand,
    MagnifyingGlass,
    MagnifyingGlassMinus,
    MagnifyingGlassPlus,
    MicrosoftPowerpointLogo,
    Minus,
    MinusCircle,
    Monitor,
    Moon,
    Notepad,
    PaintBrush,
    Palette,
    Path,
    Pause,
    PencilSimple,
    Play,
    PlayCircle,
    Plus,
    PlusCircle,
    PlusSquare,
    Presentation,
    Question,
    RadioButton,
    Rectangle,
    Robot,
    Rows,
    Selection,
    SelectionPlus,
    Shapes,
    SidebarSimple,
    SkipBack,
    SkipForward,
    SlidersHorizontal,
    SortAscending,
    Sparkle,
    SpeakerHigh,
    SpeakerLow,
    SpeakerSlash,
    Square,
    SquaresFour,
    Stack,
    Subtitles,
    Sun,
    Target,
    TextAa,
    TextAlignCenter,
    TextAlignLeft,
    TextAlignRight,
    TextB,
    TextColumns,
    TextItalic,
    TextT,
    TextUnderline,
    Timer,
    Trash,
    Trophy,
    UploadSimple,
    VideoCamera,
    Warning,
    WarningCircle,
    X,
    XCircle,
    XSquare,
}

impl Icon {
    /// Every icon, in list order.
    pub const ALL: &'static [Icon] = &[
        Icon::ArrowClockwise,
        Icon::ArrowCounterClockwise,
        Icon::ArrowDown,
        Icon::ArrowLeft,
        Icon::ArrowRight,
        Icon::ArrowSquareOut,
        Icon::ArrowUp,
        Icon::ArrowsDownUp,
        Icon::ArrowsIn,
        Icon::ArrowsOut,
        Icon::Article,
        Icon::BookOpen,
        Icon::BracketsCurly,
        Icon::CaretDown,
        Icon::CaretLeft,
        Icon::CaretRight,
        Icon::CaretUp,
        Icon::Cards,
        Icon::Check,
        Icon::CheckCircle,
        Icon::CheckSquare,
        Icon::Circle,
        Icon::Clock,
        Icon::Copy,
        Icon::CornersIn,
        Icon::CornersOut,
        Icon::Cursor,
        Icon::CursorClick,
        Icon::DotsThree,
        Icon::DotsThreeVertical,
        Icon::Drop,
        Icon::Export,
        Icon::Eye,
        Icon::EyeSlash,
        Icon::FilePlus,
        Icon::FilmStrip,
        Icon::Flag,
        Icon::FloppyDisk,
        Icon::FolderOpen,
        Icon::FrameCorners,
        Icon::Gear,
        Icon::GraduationCap,
        Icon::HandGrabbing,
        Icon::HandPointing,
        Icon::Headphones,
        Icon::House,
        Icon::Image,
        Icon::Info,
        Icon::Keyboard,
        Icon::Layout,
        Icon::Lightning,
        Icon::Link,
        Icon::List,
        Icon::ListBullets,
        Icon::ListChecks,
        Icon::ListNumbers,
        Icon::Lock,
        Icon::LockOpen,
        Icon::LockSimple,
        Icon::MagicWand,
        Icon::MagnifyingGlass,
        Icon::MagnifyingGlassMinus,
        Icon::MagnifyingGlassPlus,
        Icon::MicrosoftPowerpointLogo,
        Icon::Minus,
        Icon::MinusCircle,
        Icon::Monitor,
        Icon::Moon,
        Icon::Notepad,
        Icon::PaintBrush,
        Icon::Palette,
        Icon::Path,
        Icon::Pause,
        Icon::PencilSimple,
        Icon::Play,
        Icon::PlayCircle,
        Icon::Plus,
        Icon::PlusCircle,
        Icon::PlusSquare,
        Icon::Presentation,
        Icon::Question,
        Icon::RadioButton,
        Icon::Rectangle,
        Icon::Robot,
        Icon::Rows,
        Icon::Selection,
        Icon::SelectionPlus,
        Icon::Shapes,
        Icon::SidebarSimple,
        Icon::SkipBack,
        Icon::SkipForward,
        Icon::SlidersHorizontal,
        Icon::SortAscending,
        Icon::Sparkle,
        Icon::SpeakerHigh,
        Icon::SpeakerLow,
        Icon::SpeakerSlash,
        Icon::Square,
        Icon::SquaresFour,
        Icon::Stack,
        Icon::Subtitles,
        Icon::Sun,
        Icon::Target,
        Icon::TextAa,
        Icon::TextAlignCenter,
        Icon::TextAlignLeft,
        Icon::TextAlignRight,
        Icon::TextB,
        Icon::TextColumns,
        Icon::TextItalic,
        Icon::TextT,
        Icon::TextUnderline,
        Icon::Timer,
        Icon::Trash,
        Icon::Trophy,
        Icon::UploadSimple,
        Icon::VideoCamera,
        Icon::Warning,
        Icon::WarningCircle,
        Icon::X,
        Icon::XCircle,
        Icon::XSquare,
    ];

    /// The kebab-case Phosphor name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Icon::ArrowClockwise => "arrow-clockwise",
            Icon::ArrowCounterClockwise => "arrow-counter-clockwise",
            Icon::ArrowDown => "arrow-down",
            Icon::ArrowLeft => "arrow-left",
            Icon::ArrowRight => "arrow-right",
            Icon::ArrowSquareOut => "arrow-square-out",
            Icon::ArrowUp => "arrow-up",
            Icon::ArrowsDownUp => "arrows-down-up",
            Icon::ArrowsIn => "arrows-in",
            Icon::ArrowsOut => "arrows-out",
            Icon::Article => "article",
            Icon::BookOpen => "book-open",
            Icon::BracketsCurly => "brackets-curly",
            Icon::CaretDown => "caret-down",
            Icon::CaretLeft => "caret-left",
            Icon::CaretRight => "caret-right",
            Icon::CaretUp => "caret-up",
            Icon::Cards => "cards",
            Icon::Check => "check",
            Icon::CheckCircle => "check-circle",
            Icon::CheckSquare => "check-square",
            Icon::Circle => "circle",
            Icon::Clock => "clock",
            Icon::Copy => "copy",
            Icon::CornersIn => "corners-in",
            Icon::CornersOut => "corners-out",
            Icon::Cursor => "cursor",
            Icon::CursorClick => "cursor-click",
            Icon::DotsThree => "dots-three",
            Icon::DotsThreeVertical => "dots-three-vertical",
            Icon::Drop => "drop",
            Icon::Export => "export",
            Icon::Eye => "eye",
            Icon::EyeSlash => "eye-slash",
            Icon::FilePlus => "file-plus",
            Icon::FilmStrip => "film-strip",
            Icon::Flag => "flag",
            Icon::FloppyDisk => "floppy-disk",
            Icon::FolderOpen => "folder-open",
            Icon::FrameCorners => "frame-corners",
            Icon::Gear => "gear",
            Icon::GraduationCap => "graduation-cap",
            Icon::HandGrabbing => "hand-grabbing",
            Icon::HandPointing => "hand-pointing",
            Icon::Headphones => "headphones",
            Icon::House => "house",
            Icon::Image => "image",
            Icon::Info => "info",
            Icon::Keyboard => "keyboard",
            Icon::Layout => "layout",
            Icon::Lightning => "lightning",
            Icon::Link => "link",
            Icon::List => "list",
            Icon::ListBullets => "list-bullets",
            Icon::ListChecks => "list-checks",
            Icon::ListNumbers => "list-numbers",
            Icon::Lock => "lock",
            Icon::LockOpen => "lock-open",
            Icon::LockSimple => "lock-simple",
            Icon::MagicWand => "magic-wand",
            Icon::MagnifyingGlass => "magnifying-glass",
            Icon::MagnifyingGlassMinus => "magnifying-glass-minus",
            Icon::MagnifyingGlassPlus => "magnifying-glass-plus",
            Icon::MicrosoftPowerpointLogo => "microsoft-powerpoint-logo",
            Icon::Minus => "minus",
            Icon::MinusCircle => "minus-circle",
            Icon::Monitor => "monitor",
            Icon::Moon => "moon",
            Icon::Notepad => "notepad",
            Icon::PaintBrush => "paint-brush",
            Icon::Palette => "palette",
            Icon::Path => "path",
            Icon::Pause => "pause",
            Icon::PencilSimple => "pencil-simple",
            Icon::Play => "play",
            Icon::PlayCircle => "play-circle",
            Icon::Plus => "plus",
            Icon::PlusCircle => "plus-circle",
            Icon::PlusSquare => "plus-square",
            Icon::Presentation => "presentation",
            Icon::Question => "question",
            Icon::RadioButton => "radio-button",
            Icon::Rectangle => "rectangle",
            Icon::Robot => "robot",
            Icon::Rows => "rows",
            Icon::Selection => "selection",
            Icon::SelectionPlus => "selection-plus",
            Icon::Shapes => "shapes",
            Icon::SidebarSimple => "sidebar-simple",
            Icon::SkipBack => "skip-back",
            Icon::SkipForward => "skip-forward",
            Icon::SlidersHorizontal => "sliders-horizontal",
            Icon::SortAscending => "sort-ascending",
            Icon::Sparkle => "sparkle",
            Icon::SpeakerHigh => "speaker-high",
            Icon::SpeakerLow => "speaker-low",
            Icon::SpeakerSlash => "speaker-slash",
            Icon::Square => "square",
            Icon::SquaresFour => "squares-four",
            Icon::Stack => "stack",
            Icon::Subtitles => "subtitles",
            Icon::Sun => "sun",
            Icon::Target => "target",
            Icon::TextAa => "text-aa",
            Icon::TextAlignCenter => "text-align-center",
            Icon::TextAlignLeft => "text-align-left",
            Icon::TextAlignRight => "text-align-right",
            Icon::TextB => "text-b",
            Icon::TextColumns => "text-columns",
            Icon::TextItalic => "text-italic",
            Icon::TextT => "text-t",
            Icon::TextUnderline => "text-underline",
            Icon::Timer => "timer",
            Icon::Trash => "trash",
            Icon::Trophy => "trophy",
            Icon::UploadSimple => "upload-simple",
            Icon::VideoCamera => "video-camera",
            Icon::Warning => "warning",
            Icon::WarningCircle => "warning-circle",
            Icon::X => "x",
            Icon::XCircle => "x-circle",
            Icon::XSquare => "x-square",
        }
    }

    /// Resolve an icon from its kebab-case Phosphor name.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Icon> {
        match name {
            "arrow-clockwise" => Some(Icon::ArrowClockwise),
            "arrow-counter-clockwise" => Some(Icon::ArrowCounterClockwise),
            "arrow-down" => Some(Icon::ArrowDown),
            "arrow-left" => Some(Icon::ArrowLeft),
            "arrow-right" => Some(Icon::ArrowRight),
            "arrow-square-out" => Some(Icon::ArrowSquareOut),
            "arrow-up" => Some(Icon::ArrowUp),
            "arrows-down-up" => Some(Icon::ArrowsDownUp),
            "arrows-in" => Some(Icon::ArrowsIn),
            "arrows-out" => Some(Icon::ArrowsOut),
            "article" => Some(Icon::Article),
            "book-open" => Some(Icon::BookOpen),
            "brackets-curly" => Some(Icon::BracketsCurly),
            "caret-down" => Some(Icon::CaretDown),
            "caret-left" => Some(Icon::CaretLeft),
            "caret-right" => Some(Icon::CaretRight),
            "caret-up" => Some(Icon::CaretUp),
            "cards" => Some(Icon::Cards),
            "check" => Some(Icon::Check),
            "check-circle" => Some(Icon::CheckCircle),
            "check-square" => Some(Icon::CheckSquare),
            "circle" => Some(Icon::Circle),
            "clock" => Some(Icon::Clock),
            "copy" => Some(Icon::Copy),
            "corners-in" => Some(Icon::CornersIn),
            "corners-out" => Some(Icon::CornersOut),
            "cursor" => Some(Icon::Cursor),
            "cursor-click" => Some(Icon::CursorClick),
            "dots-three" => Some(Icon::DotsThree),
            "dots-three-vertical" => Some(Icon::DotsThreeVertical),
            "drop" => Some(Icon::Drop),
            "export" => Some(Icon::Export),
            "eye" => Some(Icon::Eye),
            "eye-slash" => Some(Icon::EyeSlash),
            "file-plus" => Some(Icon::FilePlus),
            "film-strip" => Some(Icon::FilmStrip),
            "flag" => Some(Icon::Flag),
            "floppy-disk" => Some(Icon::FloppyDisk),
            "folder-open" => Some(Icon::FolderOpen),
            "frame-corners" => Some(Icon::FrameCorners),
            "gear" => Some(Icon::Gear),
            "graduation-cap" => Some(Icon::GraduationCap),
            "hand-grabbing" => Some(Icon::HandGrabbing),
            "hand-pointing" => Some(Icon::HandPointing),
            "headphones" => Some(Icon::Headphones),
            "house" => Some(Icon::House),
            "image" => Some(Icon::Image),
            "info" => Some(Icon::Info),
            "keyboard" => Some(Icon::Keyboard),
            "layout" => Some(Icon::Layout),
            "lightning" => Some(Icon::Lightning),
            "link" => Some(Icon::Link),
            "list" => Some(Icon::List),
            "list-bullets" => Some(Icon::ListBullets),
            "list-checks" => Some(Icon::ListChecks),
            "list-numbers" => Some(Icon::ListNumbers),
            "lock" => Some(Icon::Lock),
            "lock-open" => Some(Icon::LockOpen),
            "lock-simple" => Some(Icon::LockSimple),
            "magic-wand" => Some(Icon::MagicWand),
            "magnifying-glass" => Some(Icon::MagnifyingGlass),
            "magnifying-glass-minus" => Some(Icon::MagnifyingGlassMinus),
            "magnifying-glass-plus" => Some(Icon::MagnifyingGlassPlus),
            "microsoft-powerpoint-logo" => Some(Icon::MicrosoftPowerpointLogo),
            "minus" => Some(Icon::Minus),
            "minus-circle" => Some(Icon::MinusCircle),
            "monitor" => Some(Icon::Monitor),
            "moon" => Some(Icon::Moon),
            "notepad" => Some(Icon::Notepad),
            "paint-brush" => Some(Icon::PaintBrush),
            "palette" => Some(Icon::Palette),
            "path" => Some(Icon::Path),
            "pause" => Some(Icon::Pause),
            "pencil-simple" => Some(Icon::PencilSimple),
            "play" => Some(Icon::Play),
            "play-circle" => Some(Icon::PlayCircle),
            "plus" => Some(Icon::Plus),
            "plus-circle" => Some(Icon::PlusCircle),
            "plus-square" => Some(Icon::PlusSquare),
            "presentation" => Some(Icon::Presentation),
            "question" => Some(Icon::Question),
            "radio-button" => Some(Icon::RadioButton),
            "rectangle" => Some(Icon::Rectangle),
            "robot" => Some(Icon::Robot),
            "rows" => Some(Icon::Rows),
            "selection" => Some(Icon::Selection),
            "selection-plus" => Some(Icon::SelectionPlus),
            "shapes" => Some(Icon::Shapes),
            "sidebar-simple" => Some(Icon::SidebarSimple),
            "skip-back" => Some(Icon::SkipBack),
            "skip-forward" => Some(Icon::SkipForward),
            "sliders-horizontal" => Some(Icon::SlidersHorizontal),
            "sort-ascending" => Some(Icon::SortAscending),
            "sparkle" => Some(Icon::Sparkle),
            "speaker-high" => Some(Icon::SpeakerHigh),
            "speaker-low" => Some(Icon::SpeakerLow),
            "speaker-slash" => Some(Icon::SpeakerSlash),
            "square" => Some(Icon::Square),
            "squares-four" => Some(Icon::SquaresFour),
            "stack" => Some(Icon::Stack),
            "subtitles" => Some(Icon::Subtitles),
            "sun" => Some(Icon::Sun),
            "target" => Some(Icon::Target),
            "text-aa" => Some(Icon::TextAa),
            "text-align-center" => Some(Icon::TextAlignCenter),
            "text-align-left" => Some(Icon::TextAlignLeft),
            "text-align-right" => Some(Icon::TextAlignRight),
            "text-b" => Some(Icon::TextB),
            "text-columns" => Some(Icon::TextColumns),
            "text-italic" => Some(Icon::TextItalic),
            "text-t" => Some(Icon::TextT),
            "text-underline" => Some(Icon::TextUnderline),
            "timer" => Some(Icon::Timer),
            "trash" => Some(Icon::Trash),
            "trophy" => Some(Icon::Trophy),
            "upload-simple" => Some(Icon::UploadSimple),
            "video-camera" => Some(Icon::VideoCamera),
            "warning" => Some(Icon::Warning),
            "warning-circle" => Some(Icon::WarningCircle),
            "x" => Some(Icon::X),
            "x-circle" => Some(Icon::XCircle),
            "x-square" => Some(Icon::XSquare),
            _ => None,
        }
    }

    /// The glyph codepoint (same in regular and fill).
    #[must_use]
    pub fn codepoint(self) -> char {
        match self {
            Icon::ArrowClockwise => '\u{E036}',
            Icon::ArrowCounterClockwise => '\u{E038}',
            Icon::ArrowDown => '\u{E03E}',
            Icon::ArrowLeft => '\u{E058}',
            Icon::ArrowRight => '\u{E06C}',
            Icon::ArrowSquareOut => '\u{E5DE}',
            Icon::ArrowUp => '\u{E08E}',
            Icon::ArrowsDownUp => '\u{E098}',
            Icon::ArrowsIn => '\u{E09A}',
            Icon::ArrowsOut => '\u{E0A2}',
            Icon::Article => '\u{E0A8}',
            Icon::BookOpen => '\u{E0E6}',
            Icon::BracketsCurly => '\u{E860}',
            Icon::CaretDown => '\u{E136}',
            Icon::CaretLeft => '\u{E138}',
            Icon::CaretRight => '\u{E13A}',
            Icon::CaretUp => '\u{E13C}',
            Icon::Cards => '\u{E0F8}',
            Icon::Check => '\u{E182}',
            Icon::CheckCircle => '\u{E184}',
            Icon::CheckSquare => '\u{E186}',
            Icon::Circle => '\u{E18A}',
            Icon::Clock => '\u{E19A}',
            Icon::Copy => '\u{E1CA}',
            Icon::CornersIn => '\u{E1CE}',
            Icon::CornersOut => '\u{E1D0}',
            Icon::Cursor => '\u{E1DC}',
            Icon::CursorClick => '\u{E7C8}',
            Icon::DotsThree => '\u{E1FE}',
            Icon::DotsThreeVertical => '\u{E208}',
            Icon::Drop => '\u{E210}',
            Icon::Export => '\u{EAF0}',
            Icon::Eye => '\u{E220}',
            Icon::EyeSlash => '\u{E224}',
            Icon::FilePlus => '\u{E236}',
            Icon::FilmStrip => '\u{E792}',
            Icon::Flag => '\u{E244}',
            Icon::FloppyDisk => '\u{E248}',
            Icon::FolderOpen => '\u{E256}',
            Icon::FrameCorners => '\u{E626}',
            Icon::Gear => '\u{E270}',
            Icon::GraduationCap => '\u{E62C}',
            Icon::HandGrabbing => '\u{E57C}',
            Icon::HandPointing => '\u{E29A}',
            Icon::Headphones => '\u{E2A6}',
            Icon::House => '\u{E2C2}',
            Icon::Image => '\u{E2CA}',
            Icon::Info => '\u{E2CE}',
            Icon::Keyboard => '\u{E2D8}',
            Icon::Layout => '\u{E6D6}',
            Icon::Lightning => '\u{E2DE}',
            Icon::Link => '\u{E2E2}',
            Icon::List => '\u{E2F0}',
            Icon::ListBullets => '\u{E2F2}',
            Icon::ListChecks => '\u{EADC}',
            Icon::ListNumbers => '\u{E2F6}',
            Icon::Lock => '\u{E2FA}',
            Icon::LockOpen => '\u{E306}',
            Icon::LockSimple => '\u{E308}',
            Icon::MagicWand => '\u{E6B6}',
            Icon::MagnifyingGlass => '\u{E30C}',
            Icon::MagnifyingGlassMinus => '\u{E30E}',
            Icon::MagnifyingGlassPlus => '\u{E310}',
            Icon::MicrosoftPowerpointLogo => '\u{EACE}',
            Icon::Minus => '\u{E32A}',
            Icon::MinusCircle => '\u{E32C}',
            Icon::Monitor => '\u{E32E}',
            Icon::Moon => '\u{E330}',
            Icon::Notepad => '\u{E63E}',
            Icon::PaintBrush => '\u{E6F0}',
            Icon::Palette => '\u{E6C8}',
            Icon::Path => '\u{E39C}',
            Icon::Pause => '\u{E39E}',
            Icon::PencilSimple => '\u{E3B4}',
            Icon::Play => '\u{E3D0}',
            Icon::PlayCircle => '\u{E3D2}',
            Icon::Plus => '\u{E3D4}',
            Icon::PlusCircle => '\u{E3D6}',
            Icon::PlusSquare => '\u{ED4A}',
            Icon::Presentation => '\u{E654}',
            Icon::Question => '\u{E3E8}',
            Icon::RadioButton => '\u{EB08}',
            Icon::Rectangle => '\u{E3F0}',
            Icon::Robot => '\u{E762}',
            Icon::Rows => '\u{E5A2}',
            Icon::Selection => '\u{E69A}',
            Icon::SelectionPlus => '\u{E69C}',
            Icon::Shapes => '\u{EC5E}',
            Icon::SidebarSimple => '\u{EC24}',
            Icon::SkipBack => '\u{E5A4}',
            Icon::SkipForward => '\u{E5A6}',
            Icon::SlidersHorizontal => '\u{E434}',
            Icon::SortAscending => '\u{E444}',
            Icon::Sparkle => '\u{E6A2}',
            Icon::SpeakerHigh => '\u{E44A}',
            Icon::SpeakerLow => '\u{E44C}',
            Icon::SpeakerSlash => '\u{E45A}',
            Icon::Square => '\u{E45E}',
            Icon::SquaresFour => '\u{E464}',
            Icon::Stack => '\u{E466}',
            Icon::Subtitles => '\u{E1A8}',
            Icon::Sun => '\u{E472}',
            Icon::Target => '\u{E47C}',
            Icon::TextAa => '\u{E6EE}',
            Icon::TextAlignCenter => '\u{E480}',
            Icon::TextAlignLeft => '\u{E484}',
            Icon::TextAlignRight => '\u{E486}',
            Icon::TextB => '\u{E5BE}',
            Icon::TextColumns => '\u{EC96}',
            Icon::TextItalic => '\u{E5C0}',
            Icon::TextT => '\u{E48A}',
            Icon::TextUnderline => '\u{E5C4}',
            Icon::Timer => '\u{E492}',
            Icon::Trash => '\u{E4A6}',
            Icon::Trophy => '\u{E67E}',
            Icon::UploadSimple => '\u{E4C0}',
            Icon::VideoCamera => '\u{E4DA}',
            Icon::Warning => '\u{E4E0}',
            Icon::WarningCircle => '\u{E4E2}',
            Icon::X => '\u{E4F6}',
            Icon::XCircle => '\u{E4F8}',
            Icon::XSquare => '\u{E4FA}',
        }
    }
}

/// A regular Phosphor icon formatted as [`RichText`] at the given size.
#[must_use]
pub fn icon(icon: Icon, size: f32) -> RichText {
    RichText::new(icon.codepoint().to_string())
        .family(FontFamily::Name("phosphor".into()))
        .size(size)
}

/// A filled Phosphor icon formatted as [`RichText`] at the given size (active/selected state).
#[must_use]
pub fn icon_fill(icon: Icon, size: f32) -> RichText {
    RichText::new(icon.codepoint().to_string())
        .family(FontFamily::Name("phosphor-fill".into()))
        .size(size)
}

/// A text label prefixed by an icon, suitable for buttons and menu items.
///
/// Phosphor is installed as a fallback face of the proportional family, so
/// the icon codepoint and the text label layout cleanly together.
#[must_use]
pub fn icon_text(icon: Icon, label: &str) -> RichText {
    RichText::new(format!("{} {label}", icon.codepoint()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::FontId;

    const CODEPOINTS_TXT: &str = include_str!("../fonts/phosphor-codepoints.txt");

    #[test]
    fn every_variant_matches_codepoints_file() {
        let mut count = 0;
        for line in CODEPOINTS_TXT.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.split_whitespace();
            let name = parts.next().expect("icon name");
            let hex_str = parts.next().expect("hex codepoint");
            let hex = hex_str
                .strip_prefix("U+")
                .expect("U+ prefix on codepoint hex");
            let code = u32::from_str_radix(hex, 16).expect("valid hex");
            let expected_char = char::from_u32(code).expect("valid unicode scalar");

            let icon = Icon::from_name(name)
                .unwrap_or_else(|| panic!("missing Icon variant for '{name}'"));
            assert_eq!(
                icon.codepoint(),
                expected_char,
                "mismatched codepoint for {name}"
            );
            assert_eq!(icon.name(), name, "mismatched name() for {name}");
            count += 1;
        }

        assert_eq!(
            Icon::ALL.len(),
            count,
            "ALL.len() does not match line count in phosphor-codepoints.txt"
        );
    }

    #[test]
    fn icon_glyph_resolution_with_installed_fonts() {
        let ctx = egui::Context::default();
        crate::install_fonts(&ctx);

        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let phosphor_family = FontFamily::Name("phosphor".into());
            let phosphor_fill_family = FontFamily::Name("phosphor-fill".into());

            let play_cp = Icon::Play.codepoint();
            let font_regular = FontId::new(18.0, phosphor_family);
            let font_fill = FontId::new(18.0, phosphor_fill_family);
            let font_prop = FontId::proportional(18.0);

            // 1. Phosphor regular has the glyph and non-zero width
            let has_regular = ui.ctx().fonts_mut(|f| f.has_glyph(&font_regular, play_cp));
            assert!(has_regular, "phosphor font must have Play glyph");
            let width_regular = ui
                .ctx()
                .fonts_mut(|f| f.glyph_width(&font_regular, play_cp));
            assert!(
                width_regular > 0.0,
                "Play glyph in phosphor regular must have non-zero width"
            );

            // Layout test: must produce non-zero galley width and not the replacement glyph
            let galley = ui.painter().layout_no_wrap(
                play_cp.to_string(),
                font_regular,
                egui::Color32::WHITE,
            );
            assert!(galley.size().x > 0.0);
            assert_ne!(play_cp, '\u{FFFD}');

            // 2. Phosphor fill has the glyph and non-zero width
            let has_fill = ui.ctx().fonts_mut(|f| f.has_glyph(&font_fill, play_cp));
            assert!(has_fill, "phosphor-fill font must have Play glyph");
            let width_fill = ui.ctx().fonts_mut(|f| f.glyph_width(&font_fill, play_cp));
            assert!(
                width_fill > 0.0,
                "Play glyph in phosphor fill must have non-zero width"
            );

            // 3. Proportional fallback resolves the icon glyph
            let has_proportional = ui.ctx().fonts_mut(|f| f.has_glyph(&font_prop, play_cp));
            assert!(
                has_proportional,
                "Proportional family must resolve Play icon glyph through phosphor fallback"
            );
            let width_prop = ui.ctx().fonts_mut(|f| f.glyph_width(&font_prop, play_cp));
            assert!(
                width_prop > 0.0,
                "Play glyph in Proportional must have non-zero width"
            );
        });
        output.drop_without_applying_deltas();
    }
}
