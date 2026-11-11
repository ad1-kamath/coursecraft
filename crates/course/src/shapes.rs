//! The shape catalog: every [`ShapeKind`], its label, category, default
//! placement shape and the DrawingML `prstGeom` names it imports from.
//!
//! Adding a kind is one enum variant, one row here and one geometry arm in
//! `coursecraft-render`.

use std::f32::consts::{PI, TAU};

use crate::ShapeKind;

/// A group in the shape gallery, in the order the gallery shows them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ShapeCategory {
    Lines,
    Rectangles,
    BasicShapes,
    BlockArrows,
    EquationShapes,
    Flowchart,
    StarsAndBanners,
    Callouts,
    ActionButtons,
    Learning,
}

impl ShapeCategory {
    /// Every category, in gallery order.
    pub const ALL: &'static [Self] = &[
        Self::Lines,
        Self::Rectangles,
        Self::BasicShapes,
        Self::BlockArrows,
        Self::EquationShapes,
        Self::Flowchart,
        Self::StarsAndBanners,
        Self::Callouts,
        Self::ActionButtons,
        Self::Learning,
    ];

    /// Plain-English category label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Lines => "Lines",
            Self::Rectangles => "Rectangles",
            Self::BasicShapes => "Basic Shapes",
            Self::BlockArrows => "Block Arrows",
            Self::EquationShapes => "Equation Shapes",
            Self::Flowchart => "Flowchart",
            Self::StarsAndBanners => "Stars and Banners",
            Self::Callouts => "Callouts",
            Self::ActionButtons => "Action Buttons",
            Self::Learning => "Learning",
        }
    }
}

/// One row of the shape catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShapeInfo {
    pub kind: ShapeKind,
    /// Plain English, sentence case ("Right arrow").
    pub label: &'static str,
    pub category: ShapeCategory,
    /// Default placement is 160×160 instead of 200×120.
    pub square: bool,
    /// Default placement is 120×200, taller than wide. Vertical arrows use it.
    pub tall: bool,
    /// DrawingML `prstGeom/@prst` names that import as this kind.
    pub ooxml: &'static [&'static str],
}

const fn row(
    kind: ShapeKind,
    label: &'static str,
    category: ShapeCategory,
    square: bool,
    tall: bool,
    ooxml: &'static [&'static str],
) -> ShapeInfo {
    ShapeInfo {
        kind,
        label,
        category,
        square,
        tall,
        ooxml,
    }
}

/// Every kind with exactly one row, in gallery order.
pub const SHAPE_CATALOG: &[ShapeInfo] = &[
    // Lines
    row(
        ShapeKind::LineHorizontal,
        "Horizontal line",
        ShapeCategory::Lines,
        false,
        false,
        &["line"],
    ),
    row(
        ShapeKind::LineVertical,
        "Vertical line",
        ShapeCategory::Lines,
        false,
        true,
        &[],
    ),
    row(
        ShapeKind::LineDiagonalDown,
        "Diagonal line down",
        ShapeCategory::Lines,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::LineDiagonalUp,
        "Diagonal line up",
        ShapeCategory::Lines,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::LineArrowRight,
        "Right line arrow",
        ShapeCategory::Lines,
        false,
        false,
        &["straightConnector1"],
    ),
    row(
        ShapeKind::LineArrowLeft,
        "Left line arrow",
        ShapeCategory::Lines,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::LineArrowUp,
        "Up line arrow",
        ShapeCategory::Lines,
        false,
        true,
        &[],
    ),
    row(
        ShapeKind::LineArrowDown,
        "Down line arrow",
        ShapeCategory::Lines,
        false,
        true,
        &[],
    ),
    row(
        ShapeKind::LineArrowBothHorizontal,
        "Double line arrow",
        ShapeCategory::Lines,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::LineArrowBothVertical,
        "Double vertical line arrow",
        ShapeCategory::Lines,
        false,
        true,
        &[],
    ),
    // Rectangles
    row(
        ShapeKind::Rectangle,
        "Rectangle",
        ShapeCategory::Rectangles,
        true,
        false,
        &["rect"],
    ),
    row(
        ShapeKind::RoundedRectangle,
        "Rounded rectangle",
        ShapeCategory::Rectangles,
        true,
        false,
        &["roundRect"],
    ),
    row(
        ShapeKind::SnipSingleCorner,
        "Snip single corner",
        ShapeCategory::Rectangles,
        true,
        false,
        &["snip1Rect"],
    ),
    row(
        ShapeKind::SnipSameSide,
        "Snip same side",
        ShapeCategory::Rectangles,
        true,
        false,
        &["snip2SameRect"],
    ),
    row(
        ShapeKind::SnipDiagonal,
        "Snip diagonal",
        ShapeCategory::Rectangles,
        true,
        false,
        &["snip2DiagRect"],
    ),
    row(
        ShapeKind::SnipRound,
        "Snip round",
        ShapeCategory::Rectangles,
        true,
        false,
        &["snipRoundRect"],
    ),
    row(
        ShapeKind::RoundSingleCorner,
        "Round single corner",
        ShapeCategory::Rectangles,
        true,
        false,
        &["round1Rect"],
    ),
    row(
        ShapeKind::RoundSameSide,
        "Round same side",
        ShapeCategory::Rectangles,
        true,
        false,
        &["round2SameRect"],
    ),
    row(
        ShapeKind::RoundDiagonal,
        "Round diagonal",
        ShapeCategory::Rectangles,
        true,
        false,
        &["round2DiagRect"],
    ),
    // Basic shapes
    row(
        ShapeKind::Ellipse,
        "Ellipse",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["ellipse"],
    ),
    row(
        ShapeKind::Triangle,
        "Triangle",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["triangle"],
    ),
    row(
        ShapeKind::RightTriangle,
        "Right triangle",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["rtTriangle"],
    ),
    row(
        ShapeKind::Parallelogram,
        "Parallelogram",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["parallelogram"],
    ),
    row(
        ShapeKind::Trapezoid,
        "Trapezoid",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["trapezoid"],
    ),
    row(
        ShapeKind::Diamond,
        "Diamond",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["diamond"],
    ),
    row(
        ShapeKind::Pentagon,
        "Pentagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["pentagon"],
    ),
    row(
        ShapeKind::Hexagon,
        "Hexagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["hexagon"],
    ),
    row(
        ShapeKind::Heptagon,
        "Heptagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["heptagon"],
    ),
    row(
        ShapeKind::Octagon,
        "Octagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["octagon"],
    ),
    row(
        ShapeKind::Decagon,
        "Decagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["decagon"],
    ),
    row(
        ShapeKind::Dodecagon,
        "Dodecagon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["dodecagon"],
    ),
    row(
        ShapeKind::Pie,
        "Pie",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["pie"],
    ),
    row(
        ShapeKind::Chord,
        "Chord",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["chord"],
    ),
    row(
        ShapeKind::Teardrop,
        "Teardrop",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["teardrop"],
    ),
    row(
        ShapeKind::Frame,
        "Frame",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["frame"],
    ),
    row(
        ShapeKind::HalfFrame,
        "Half frame",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["halfFrame"],
    ),
    row(
        ShapeKind::Corner,
        "Corner",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["corner"],
    ),
    row(
        ShapeKind::DiagonalStripe,
        "Diagonal stripe",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["diagStripe"],
    ),
    row(
        ShapeKind::Plus,
        "Plus",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["plus"],
    ),
    row(
        ShapeKind::Plaque,
        "Plaque",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["plaque"],
    ),
    row(
        ShapeKind::Can,
        "Can",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["can"],
    ),
    row(
        ShapeKind::Cube,
        "Cube",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["cube"],
    ),
    row(
        ShapeKind::Bevel,
        "Bevel",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["bevel"],
    ),
    row(
        ShapeKind::Donut,
        "Donut",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["donut"],
    ),
    row(
        ShapeKind::NoSymbol,
        "No symbol",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["noSmoking"],
    ),
    row(
        ShapeKind::BlockArc,
        "Block arc",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["blockArc"],
    ),
    row(
        ShapeKind::FoldedCorner,
        "Folded corner",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["foldedCorner"],
    ),
    row(
        ShapeKind::Smiley,
        "Smiley face",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["smileyFace"],
    ),
    row(
        ShapeKind::Heart,
        "Heart",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["heart"],
    ),
    row(
        ShapeKind::LightningBolt,
        "Lightning bolt",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["lightningBolt"],
    ),
    row(
        ShapeKind::Sun,
        "Sun",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["sun"],
    ),
    row(
        ShapeKind::Moon,
        "Moon",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["moon"],
    ),
    row(
        ShapeKind::Cloud,
        "Cloud",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["cloud"],
    ),
    row(
        ShapeKind::Arc,
        "Arc",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["arc"],
    ),
    row(
        ShapeKind::BracketPair,
        "Bracket pair",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["bracketPair"],
    ),
    row(
        ShapeKind::BracePair,
        "Brace pair",
        ShapeCategory::BasicShapes,
        false,
        false,
        &["bracePair"],
    ),
    row(
        ShapeKind::LeftBracket,
        "Left bracket",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["leftBracket"],
    ),
    row(
        ShapeKind::RightBracket,
        "Right bracket",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["rightBracket"],
    ),
    row(
        ShapeKind::LeftBrace,
        "Left brace",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["leftBrace"],
    ),
    row(
        ShapeKind::RightBrace,
        "Right brace",
        ShapeCategory::BasicShapes,
        true,
        false,
        &["rightBrace"],
    ),
    // Block arrows
    row(
        ShapeKind::ArrowRight,
        "Right arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["rightArrow"],
    ),
    row(
        ShapeKind::ArrowLeft,
        "Left arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["leftArrow"],
    ),
    row(
        ShapeKind::ArrowUp,
        "Up arrow",
        ShapeCategory::BlockArrows,
        false,
        true,
        &["upArrow"],
    ),
    row(
        ShapeKind::ArrowDown,
        "Down arrow",
        ShapeCategory::BlockArrows,
        false,
        true,
        &["downArrow"],
    ),
    row(
        ShapeKind::ArrowLeftRight,
        "Left-right arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["leftRightArrow"],
    ),
    row(
        ShapeKind::ArrowUpDown,
        "Up-down arrow",
        ShapeCategory::BlockArrows,
        false,
        true,
        &["upDownArrow"],
    ),
    row(
        ShapeKind::ArrowQuad,
        "Quad arrow",
        ShapeCategory::BlockArrows,
        true,
        false,
        &["quadArrow"],
    ),
    row(
        ShapeKind::ArrowLeftRightUp,
        "Left-right-up arrow",
        ShapeCategory::BlockArrows,
        true,
        false,
        &["leftRightUpArrow"],
    ),
    row(
        ShapeKind::ArrowBent,
        "Bent arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["bentArrow"],
    ),
    row(
        ShapeKind::ArrowUTurn,
        "U-turn arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["uturnArrow"],
    ),
    row(
        ShapeKind::ArrowLeftUp,
        "Left-up arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["leftUpArrow"],
    ),
    row(
        ShapeKind::ArrowBentUp,
        "Bent-up arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["bentUpArrow"],
    ),
    row(
        ShapeKind::ArrowCurvedRight,
        "Curved right arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["curvedRightArrow"],
    ),
    row(
        ShapeKind::ArrowCurvedLeft,
        "Curved left arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["curvedLeftArrow"],
    ),
    row(
        ShapeKind::ArrowCurvedUp,
        "Curved up arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["curvedUpArrow"],
    ),
    row(
        ShapeKind::ArrowCurvedDown,
        "Curved down arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["curvedDownArrow"],
    ),
    row(
        ShapeKind::ArrowStripedRight,
        "Striped right arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["stripedRightArrow"],
    ),
    row(
        ShapeKind::ArrowNotchedRight,
        "Notched right arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["notchedRightArrow"],
    ),
    row(
        ShapeKind::ArrowPentagon,
        "Pentagon arrow",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["homePlate"],
    ),
    row(
        ShapeKind::ArrowChevron,
        "Chevron",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["chevron"],
    ),
    row(
        ShapeKind::CalloutRightArrow,
        "Right arrow callout",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["rightArrowCallout"],
    ),
    row(
        ShapeKind::CalloutDownArrow,
        "Down arrow callout",
        ShapeCategory::BlockArrows,
        false,
        true,
        &["downArrowCallout"],
    ),
    row(
        ShapeKind::CalloutLeftArrow,
        "Left arrow callout",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["leftArrowCallout"],
    ),
    row(
        ShapeKind::CalloutUpArrow,
        "Up arrow callout",
        ShapeCategory::BlockArrows,
        false,
        true,
        &["upArrowCallout"],
    ),
    row(
        ShapeKind::CalloutLeftRightArrow,
        "Left-right arrow callout",
        ShapeCategory::BlockArrows,
        false,
        false,
        &["leftRightArrowCallout"],
    ),
    row(
        ShapeKind::CalloutQuadArrow,
        "Quad arrow callout",
        ShapeCategory::BlockArrows,
        true,
        false,
        &["quadArrowCallout"],
    ),
    row(
        ShapeKind::ArrowCircular,
        "Circular arrow",
        ShapeCategory::BlockArrows,
        true,
        false,
        &["circularArrow"],
    ),
    // Equation shapes
    row(
        ShapeKind::MathPlus,
        "Plus",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathPlus"],
    ),
    row(
        ShapeKind::MathMinus,
        "Minus",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathMinus"],
    ),
    row(
        ShapeKind::MathMultiply,
        "Multiply",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathMultiply"],
    ),
    row(
        ShapeKind::MathDivide,
        "Divide",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathDivide"],
    ),
    row(
        ShapeKind::MathEqual,
        "Equal",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathEqual"],
    ),
    row(
        ShapeKind::MathNotEqual,
        "Not equal",
        ShapeCategory::EquationShapes,
        true,
        false,
        &["mathNotEqual"],
    ),
    // Flowchart
    row(
        ShapeKind::FlowProcess,
        "Process",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartProcess"],
    ),
    row(
        ShapeKind::FlowAlternateProcess,
        "Alternate process",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartAlternateProcess"],
    ),
    row(
        ShapeKind::FlowDecision,
        "Decision",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartDecision"],
    ),
    row(
        ShapeKind::FlowInputOutput,
        "Input or output",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartInputOutput"],
    ),
    row(
        ShapeKind::FlowPredefinedProcess,
        "Predefined process",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartPredefinedProcess"],
    ),
    row(
        ShapeKind::FlowInternalStorage,
        "Internal storage",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartInternalStorage"],
    ),
    row(
        ShapeKind::FlowDocument,
        "Document",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartDocument"],
    ),
    row(
        ShapeKind::FlowMultidocument,
        "Multidocument",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartMultidocument"],
    ),
    row(
        ShapeKind::FlowTerminator,
        "Terminator",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartTerminator"],
    ),
    row(
        ShapeKind::FlowPreparation,
        "Preparation",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartPreparation"],
    ),
    row(
        ShapeKind::FlowManualInput,
        "Manual input",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartManualInput"],
    ),
    row(
        ShapeKind::FlowManualOperation,
        "Manual operation",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartManualOperation"],
    ),
    row(
        ShapeKind::FlowConnector,
        "Connector",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartConnector"],
    ),
    row(
        ShapeKind::FlowOffpageConnector,
        "Off-page connector",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartOffpageConnector"],
    ),
    row(
        ShapeKind::FlowCard,
        "Card",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartPunchedCard"],
    ),
    row(
        ShapeKind::FlowPunchedTape,
        "Punched tape",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartPunchedTape"],
    ),
    row(
        ShapeKind::FlowSummingJunction,
        "Summing junction",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartSummingJunction"],
    ),
    row(
        ShapeKind::FlowOr,
        "Or",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartOr"],
    ),
    row(
        ShapeKind::FlowCollate,
        "Collate",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartCollate"],
    ),
    row(
        ShapeKind::FlowSort,
        "Sort",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartSort"],
    ),
    row(
        ShapeKind::FlowExtract,
        "Extract",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartExtract"],
    ),
    row(
        ShapeKind::FlowMerge,
        "Merge",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartMerge"],
    ),
    row(
        ShapeKind::FlowStoredData,
        "Stored data",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartOnlineStorage"],
    ),
    row(
        ShapeKind::FlowDelay,
        "Delay",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartDelay"],
    ),
    row(
        ShapeKind::FlowSequentialAccess,
        "Sequential access",
        ShapeCategory::Flowchart,
        true,
        false,
        &["flowChartMagneticTape"],
    ),
    row(
        ShapeKind::FlowMagneticDisk,
        "Magnetic disk",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartMagneticDisk"],
    ),
    row(
        ShapeKind::FlowDirectAccess,
        "Direct access",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartMagneticDrum"],
    ),
    row(
        ShapeKind::FlowDisplay,
        "Display",
        ShapeCategory::Flowchart,
        false,
        false,
        &["flowChartDisplay"],
    ),
    // Stars and banners
    row(
        ShapeKind::Explosion1,
        "Explosion 1",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["irregularSeal1"],
    ),
    row(
        ShapeKind::Explosion2,
        "Explosion 2",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["irregularSeal2"],
    ),
    row(
        ShapeKind::Star4,
        "4-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star4"],
    ),
    row(
        ShapeKind::Star5,
        "5-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star5"],
    ),
    row(
        ShapeKind::Star6,
        "6-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star6"],
    ),
    row(
        ShapeKind::Star7,
        "7-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star7"],
    ),
    row(
        ShapeKind::Star8,
        "8-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star8"],
    ),
    row(
        ShapeKind::Star10,
        "10-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star10"],
    ),
    row(
        ShapeKind::Star12,
        "12-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star12"],
    ),
    row(
        ShapeKind::Star16,
        "16-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star16"],
    ),
    row(
        ShapeKind::Star24,
        "24-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star24"],
    ),
    row(
        ShapeKind::Star32,
        "32-point star",
        ShapeCategory::StarsAndBanners,
        true,
        false,
        &["star32"],
    ),
    row(
        ShapeKind::RibbonUp,
        "Ribbon up",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["ribbon2"],
    ),
    row(
        ShapeKind::RibbonDown,
        "Ribbon down",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["ribbon"],
    ),
    row(
        ShapeKind::CurvedRibbonUp,
        "Curved ribbon up",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["ellipseRibbon2"],
    ),
    row(
        ShapeKind::CurvedRibbonDown,
        "Curved ribbon down",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["ellipseRibbon"],
    ),
    row(
        ShapeKind::ScrollVertical,
        "Vertical scroll",
        ShapeCategory::StarsAndBanners,
        false,
        true,
        &["verticalScroll"],
    ),
    row(
        ShapeKind::ScrollHorizontal,
        "Horizontal scroll",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["horizontalScroll"],
    ),
    row(
        ShapeKind::Wave,
        "Wave",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["wave"],
    ),
    row(
        ShapeKind::DoubleWave,
        "Double wave",
        ShapeCategory::StarsAndBanners,
        false,
        false,
        &["doubleWave"],
    ),
    // Callouts
    row(
        ShapeKind::CalloutRectangle,
        "Rectangular callout",
        ShapeCategory::Callouts,
        false,
        false,
        &["wedgeRectCallout"],
    ),
    row(
        ShapeKind::CalloutRounded,
        "Rounded callout",
        ShapeCategory::Callouts,
        false,
        false,
        &["wedgeRoundRectCallout"],
    ),
    row(
        ShapeKind::CalloutOval,
        "Oval callout",
        ShapeCategory::Callouts,
        false,
        false,
        &["wedgeEllipseCallout"],
    ),
    row(
        ShapeKind::CalloutCloud,
        "Cloud callout",
        ShapeCategory::Callouts,
        false,
        false,
        &["cloudCallout"],
    ),
    row(
        ShapeKind::CalloutLine,
        "Line callout",
        ShapeCategory::Callouts,
        false,
        false,
        &[
            "borderCallout1",
            "callout1",
            "accentCallout1",
            "accentBorderCallout1",
        ],
    ),
    row(
        ShapeKind::CalloutLineBent,
        "Bent line callout",
        ShapeCategory::Callouts,
        false,
        false,
        &[
            "borderCallout2",
            "callout2",
            "accentCallout2",
            "accentBorderCallout2",
        ],
    ),
    row(
        ShapeKind::CalloutLineDoubleBent,
        "Double-bent line callout",
        ShapeCategory::Callouts,
        false,
        false,
        &[
            "borderCallout3",
            "callout3",
            "accentCallout3",
            "accentBorderCallout3",
        ],
    ),
    // Action buttons
    row(
        ShapeKind::ActionBlank,
        "Blank action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonBlank"],
    ),
    row(
        ShapeKind::ActionHome,
        "Home action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonHome"],
    ),
    row(
        ShapeKind::ActionHelp,
        "Help action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonHelp"],
    ),
    row(
        ShapeKind::ActionInformation,
        "Information action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonInformation"],
    ),
    row(
        ShapeKind::ActionBack,
        "Back action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonBackPrevious"],
    ),
    row(
        ShapeKind::ActionForward,
        "Forward action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonForwardNext"],
    ),
    row(
        ShapeKind::ActionBeginning,
        "Beginning action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonBeginning"],
    ),
    row(
        ShapeKind::ActionEnd,
        "End action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonEnd"],
    ),
    row(
        ShapeKind::ActionReturn,
        "Return action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonReturn"],
    ),
    row(
        ShapeKind::ActionDocument,
        "Document action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonDocument"],
    ),
    row(
        ShapeKind::ActionSound,
        "Sound action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonSound"],
    ),
    row(
        ShapeKind::ActionMovie,
        "Movie action button",
        ShapeCategory::ActionButtons,
        true,
        false,
        &["actionButtonMovie"],
    ),
    // Learning
    row(
        ShapeKind::CheckMark,
        "Check mark",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::CrossMark,
        "Cross mark",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::CheckCircle,
        "Check circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::CrossCircle,
        "Cross circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::InfoCircle,
        "Info circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::QuestionCircle,
        "Question circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::WarningTriangle,
        "Warning",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Lightbulb,
        "Lightbulb",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Target,
        "Target",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Trophy,
        "Trophy",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Medal,
        "Medal",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Flag,
        "Flag",
        ShapeCategory::Learning,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::LocationPin,
        "Location pin",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Clock,
        "Clock",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Lock,
        "Lock",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Key,
        "Key",
        ShapeCategory::Learning,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::Shield,
        "Shield",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Book,
        "Book",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::GraduationCap,
        "Graduation cap",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Person,
        "Person",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::People,
        "People",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::ThumbsUp,
        "Thumbs up",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::SpeechBubble,
        "Speech bubble",
        ShapeCategory::Learning,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::ThoughtBubble,
        "Thought bubble",
        ShapeCategory::Learning,
        false,
        false,
        &[],
    ),
    row(
        ShapeKind::PlayCircle,
        "Play circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::PauseCircle,
        "Pause circle",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Gear,
        "Gear",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::Magnifier,
        "Magnifier",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
    row(
        ShapeKind::StarBadge,
        "Star badge",
        ShapeCategory::Learning,
        true,
        false,
        &[],
    ),
];

/// The catalog row for `kind`.
#[must_use]
pub fn shape_info(kind: ShapeKind) -> Option<&'static ShapeInfo> {
    SHAPE_CATALOG.iter().find(|info| info.kind == kind)
}

/// The plain-English label for `kind`, or `"Shape"` if a row is missing.
#[must_use]
pub fn shape_label(kind: ShapeKind) -> &'static str {
    shape_info(kind).map_or("Shape", |info| info.label)
}

/// The adjustment values a kind understands, with their defaults and limits.
///
/// Values are unitless. Most are fractions of `min(w, h)`; callout tail points
/// are fractions of the width and height, and angle values are radians.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdjustSpec {
    pub defaults: &'static [f32],
    pub min: &'static [f32],
    pub max: &'static [f32],
}

impl AdjustSpec {
    /// Number of values this kind uses.
    #[must_use]
    pub const fn count(&self) -> usize {
        self.defaults.len()
    }
}

const NO_ADJUST: AdjustSpec = AdjustSpec {
    defaults: &[],
    min: &[],
    max: &[],
};

const fn spec(defaults: &'static [f32], min: &'static [f32], max: &'static [f32]) -> AdjustSpec {
    AdjustSpec { defaults, min, max }
}

/// The adjustment spec for `kind`; kinds without handles get [`NO_ADJUST`].
#[must_use]
pub fn adjust_spec(kind: ShapeKind) -> AdjustSpec {
    match kind {
        ShapeKind::RoundedRectangle
        | ShapeKind::SnipSingleCorner
        | ShapeKind::SnipSameSide
        | ShapeKind::SnipDiagonal
        | ShapeKind::RoundSingleCorner
        | ShapeKind::RoundSameSide
        | ShapeKind::RoundDiagonal => spec(&[0.1], &[0.0], &[0.5]),
        ShapeKind::SnipRound => spec(&[0.1, 0.1], &[0.0, 0.0], &[0.5, 0.5]),
        ShapeKind::CalloutRectangle | ShapeKind::CalloutRounded | ShapeKind::CalloutOval => {
            spec(&[0.14, 1.0], &[-2.0, -2.0], &[3.0, 3.0])
        }
        ShapeKind::CalloutLine => spec(&[0.18, 0.97], &[-2.0, -2.0], &[3.0, 3.0]),
        ShapeKind::CalloutLineBent => spec(
            &[0.17, 0.7, 0.17, 0.97],
            &[-2.0, -2.0, -2.0, -2.0],
            &[3.0, 3.0, 3.0, 3.0],
        ),
        ShapeKind::CalloutLineDoubleBent => spec(
            &[0.13, 0.8, 0.24, 0.8, 0.24, 0.97],
            &[-2.0, -2.0, -2.0, -2.0, -2.0, -2.0],
            &[3.0, 3.0, 3.0, 3.0, 3.0, 3.0],
        ),
        ShapeKind::ArrowRight
        | ShapeKind::ArrowLeft
        | ShapeKind::ArrowUp
        | ShapeKind::ArrowDown
        | ShapeKind::ArrowLeftRight
        | ShapeKind::ArrowUpDown => spec(&[0.5, 0.25], &[0.05, 0.02], &[0.9, 0.45]),
        ShapeKind::ArrowPentagon => spec(&[0.5], &[0.05], &[0.9]),
        ShapeKind::ArrowChevron => spec(&[0.35], &[0.05], &[0.9]),
        ShapeKind::Parallelogram | ShapeKind::Trapezoid => spec(&[0.25], &[0.0], &[0.9]),
        ShapeKind::Hexagon | ShapeKind::Octagon | ShapeKind::Plus => spec(&[0.0], &[0.0], &[0.45]),
        ShapeKind::Star4 => spec(&[0.3], &[0.05], &[0.95]),
        ShapeKind::Star5 => spec(&[0.38], &[0.05], &[0.95]),
        ShapeKind::Star6 => spec(&[0.42], &[0.05], &[0.95]),
        ShapeKind::Star7 => spec(&[0.45], &[0.05], &[0.95]),
        ShapeKind::Star8 => spec(&[0.42], &[0.05], &[0.95]),
        ShapeKind::Star10 => spec(&[0.48], &[0.05], &[0.95]),
        ShapeKind::Star12 => spec(&[0.5], &[0.05], &[0.95]),
        ShapeKind::Star16 => spec(&[0.52], &[0.05], &[0.95]),
        ShapeKind::Donut => spec(&[0.45], &[0.05], &[0.9]),
        ShapeKind::BlockArc => spec(&[0.2, PI, TAU], &[0.02, -TAU, -TAU], &[0.8, TAU, TAU * 2.0]),
        ShapeKind::Pie | ShapeKind::Chord => {
            spec(&[0.0, TAU * 0.75], &[-TAU, -TAU], &[TAU, TAU * 2.0])
        }
        ShapeKind::Arc => spec(&[-PI * 0.5, 0.0], &[-TAU, -TAU], &[TAU, TAU * 2.0]),
        ShapeKind::Frame => spec(&[0.125], &[0.01], &[0.45]),
        ShapeKind::Bevel => spec(&[0.16], &[0.02], &[0.45]),
        ShapeKind::FoldedCorner => spec(&[0.28], &[0.02], &[0.45]),
        ShapeKind::Can => spec(&[0.12], &[0.01], &[0.45]),
        ShapeKind::Cube => spec(&[0.25], &[0.05], &[0.45]),
        ShapeKind::LeftBracket | ShapeKind::RightBracket => spec(&[0.3], &[0.02], &[0.9]),
        ShapeKind::LeftBrace | ShapeKind::RightBrace => spec(&[0.85], &[0.1], &[0.95]),
        _ => NO_ADJUST,
    }
}

/// Whether `kind` has any adjustment handles.
#[must_use]
pub fn shape_is_adjustable(kind: ShapeKind) -> bool {
    adjust_spec(kind).count() > 0
}

/// Clamp `values` to `kind`'s limits, filling missing values with the defaults.
///
/// An empty input stays empty, because empty is how the model stores "use the
/// defaults". Extra values are ignored and non-finite values fall back to their
/// default. The result always has exactly [`AdjustSpec::count`] values.
#[must_use]
pub fn clamp_adjust(kind: ShapeKind, values: &[f32]) -> Vec<f32> {
    if values.is_empty() {
        return Vec::new();
    }
    let spec = adjust_spec(kind);
    let count = spec.count();
    (0..count)
        .map(|i| {
            let default = spec.defaults.get(i).copied().unwrap_or(0.0);
            let min = spec.min.get(i).copied().unwrap_or(default);
            let max = spec.max.get(i).copied().unwrap_or(default);
            let raw = values.get(i).copied().unwrap_or(default);
            let value = if raw.is_finite() { raw } else { default };
            value.clamp(min.min(max), max.max(min))
        })
        .collect()
}

/// The defaults for `kind`, or an empty slice when it has no adjustments.
#[must_use]
pub fn adjust_defaults(kind: ShapeKind) -> &'static [f32] {
    adjust_spec(kind).defaults
}

/// Whether `kind` can carry text. Line kinds are strokes, not containers.
#[must_use]
pub fn shape_allows_text(kind: ShapeKind) -> bool {
    shape_info(kind).is_some_and(|info| info.category != ShapeCategory::Lines)
}

/// Map a DrawingML `prstGeom/@prst` name to a kind through the catalog.
#[must_use]
pub fn shape_kind_for_ooxml(name: &str) -> Option<ShapeKind> {
    SHAPE_CATALOG
        .iter()
        .find(|info| info.ooxml.contains(&name))
        .map(|info| info.kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_variant_has_exactly_one_catalog_row() {
        assert_eq!(SHAPE_CATALOG.len(), ShapeKind::ALL.len());
        for &kind in ShapeKind::ALL {
            let rows: Vec<&ShapeInfo> = SHAPE_CATALOG
                .iter()
                .filter(|info| info.kind == kind)
                .collect();
            assert_eq!(rows.len(), 1, "{kind:?} must have exactly one row");
        }
        for info in SHAPE_CATALOG {
            assert!(
                ShapeKind::ALL.contains(&info.kind),
                "catalog row for {:?} is not in ShapeKind::ALL",
                info.kind
            );
        }
    }

    #[test]
    fn categories_run_in_gallery_order() {
        assert_eq!(
            ShapeCategory::ALL
                .iter()
                .map(|category| category.label())
                .collect::<Vec<_>>(),
            vec![
                "Lines",
                "Rectangles",
                "Basic Shapes",
                "Block Arrows",
                "Equation Shapes",
                "Flowchart",
                "Stars and Banners",
                "Callouts",
                "Action Buttons",
                "Learning",
            ]
        );
    }

    #[test]
    fn ooxml_names_map_to_their_kind() {
        for info in SHAPE_CATALOG {
            for name in info.ooxml {
                assert_eq!(shape_kind_for_ooxml(name), Some(info.kind), "{name}");
            }
        }
        assert_eq!(
            shape_kind_for_ooxml("rightArrow"),
            Some(ShapeKind::ArrowRight)
        );
        assert_eq!(
            shape_kind_for_ooxml("round2DiagRect"),
            Some(ShapeKind::RoundDiagonal)
        );
        assert_eq!(
            shape_kind_for_ooxml("flowChartOnlineStorage"),
            Some(ShapeKind::FlowStoredData)
        );
        assert_eq!(
            shape_kind_for_ooxml("actionButtonHome"),
            Some(ShapeKind::ActionHome)
        );
        assert_eq!(shape_kind_for_ooxml("noSuchShape"), None);
    }

    #[test]
    fn ooxml_names_are_unique() {
        let mut seen: Vec<&str> = Vec::new();
        for info in SHAPE_CATALOG {
            for name in info.ooxml {
                assert!(!seen.contains(name), "{name} is claimed by two rows");
                seen.push(name);
            }
        }
    }

    #[test]
    fn vertical_arrows_default_to_a_tall_frame() {
        for kind in [
            ShapeKind::ArrowUp,
            ShapeKind::ArrowDown,
            ShapeKind::ArrowUpDown,
            ShapeKind::CalloutUpArrow,
            ShapeKind::CalloutDownArrow,
            ShapeKind::LineVertical,
            ShapeKind::LineArrowUp,
            ShapeKind::LineArrowDown,
            ShapeKind::LineArrowBothVertical,
        ] {
            let info = shape_info(kind).expect("catalog row");
            assert!(info.tall, "{kind:?} should default to a tall frame");
            assert!(!info.square, "{kind:?} cannot be square and tall");
        }
        assert!(shape_info(ShapeKind::ArrowQuad).expect("row").square);
        assert!(shape_info(ShapeKind::ArrowLeftRightUp).expect("row").square);
        assert!(shape_info(ShapeKind::CalloutQuadArrow).expect("row").square);
    }

    #[test]
    fn every_category_has_a_row() {
        for category in ShapeCategory::ALL {
            assert!(
                SHAPE_CATALOG.iter().any(|info| info.category == *category),
                "{} is empty",
                category.label()
            );
        }
    }
}
