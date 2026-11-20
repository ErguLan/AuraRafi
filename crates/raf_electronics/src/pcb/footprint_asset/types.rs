use serde::{Deserialize, Serialize};

/// A 2D point in millimeters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct PointMm {
    pub x: f64,
    pub y: f64,
}

impl PointMm {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

/// A width and height in millimeters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct SizeMm {
    pub width: f64,
    pub height: f64,
}

impl SizeMm {
    pub(super) fn is_positive_finite(self) -> bool {
        self.width.is_finite() && self.height.is_finite() && self.width > 0.0 && self.height > 0.0
    }
}

/// Axis-aligned bounds in footprint-local millimeter coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FootprintBoundsMm {
    pub min: PointMm,
    pub max: PointMm,
}

impl FootprintBoundsMm {
    pub fn width_mm(self) -> f64 {
        self.max.x - self.min.x
    }

    pub fn height_mm(self) -> f64 {
        self.max.y - self.min.y
    }

    pub fn size_mm(self) -> SizeMm {
        SizeMm {
            width: self.width_mm(),
            height: self.height_mm(),
        }
    }
}

/// A canonical KiCad layer needed by the imported physical geometry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FootprintLayer {
    FrontCopper,
    BackCopper,
    InnerCopper(String),
    AllCopper,
    FrontMask,
    BackMask,
    AllMask,
    FrontPaste,
    BackPaste,
    AllPaste,
    FrontSilkscreen,
    BackSilkscreen,
    FrontFab,
    BackFab,
    FrontCourtyard,
    BackCourtyard,
    BoardOutline,
    Other(String),
}

/// Layer assigned to a pad. Kept as an alias for a concise pad-facing API.
pub type FootprintPadLayer = FootprintLayer;

impl FootprintLayer {
    pub(super) fn from_kicad_name(name: &str) -> Self {
        match name {
            "F.Cu" => Self::FrontCopper,
            "B.Cu" => Self::BackCopper,
            "*.Cu" => Self::AllCopper,
            "F.Mask" => Self::FrontMask,
            "B.Mask" => Self::BackMask,
            "*.Mask" => Self::AllMask,
            "F.Paste" => Self::FrontPaste,
            "B.Paste" => Self::BackPaste,
            "*.Paste" => Self::AllPaste,
            "F.SilkS" => Self::FrontSilkscreen,
            "B.SilkS" => Self::BackSilkscreen,
            "F.Fab" => Self::FrontFab,
            "B.Fab" => Self::BackFab,
            "F.CrtYd" => Self::FrontCourtyard,
            "B.CrtYd" => Self::BackCourtyard,
            "Edge.Cuts" => Self::BoardOutline,
            _ if name.starts_with("In") && name.ends_with(".Cu") => {
                Self::InnerCopper(name.to_string())
            }
            _ => Self::Other(name.to_string()),
        }
    }

    pub(crate) fn is_physical(&self) -> bool {
        matches!(
            self,
            Self::FrontCopper
                | Self::BackCopper
                | Self::InnerCopper(_)
                | Self::AllCopper
                | Self::FrontMask
                | Self::BackMask
                | Self::AllMask
                | Self::FrontPaste
                | Self::BackPaste
                | Self::AllPaste
                | Self::FrontSilkscreen
                | Self::BackSilkscreen
                | Self::FrontFab
                | Self::BackFab
                | Self::FrontCourtyard
                | Self::BackCourtyard
                | Self::BoardOutline
        )
    }
}

/// Pad geometry supported by the physical footprint model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FootprintPadShape {
    Rectangle,
    Circle,
    Oval,
    RoundRect {
        corner_ratio: Option<f64>,
    },
    /// The source shape is retained and blocks manufacturing readiness.
    Unsupported {
        source_shape: String,
    },
}

/// Electrical/mechanical role of a pad.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintPadTechnology {
    Smd,
    PlatedThroughHole,
    NonPlatedThroughHole,
    /// A KiCad pad type not modeled by this crate.
    Unsupported {
        source_type: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FootprintDrillShape {
    Circular { diameter_mm: f64 },
    Oval { width_mm: f64, height_mm: f64 },
}

/// Hole size and its offset from the pad center, in millimeters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintDrill {
    pub shape: FootprintDrillShape,
    pub offset_mm: PointMm,
}

/// One physical pad from a footprint library entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintPad {
    pub number: String,
    pub position_mm: PointMm,
    pub rotation_degrees: f64,
    pub size_mm: SizeMm,
    pub shape: FootprintPadShape,
    pub technology: FootprintPadTechnology,
    pub layers: Vec<FootprintPadLayer>,
    pub drill: Option<FootprintDrill>,
    pub solder_mask_margin_mm: Option<f64>,
    pub solder_paste_margin_mm: Option<f64>,
    pub solder_paste_margin_ratio: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintTextRole {
    Reference,
    Value,
    User,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FootprintStrokeStyle {
    Default,
    Solid,
    Dashed,
    Dotted,
    DashDot,
    DashDotDot,
    Unsupported(String),
}

/// Vector geometry captured from silk, fab, courtyard, and other named layers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FootprintGraphicGeometry {
    Line {
        start_mm: PointMm,
        end_mm: PointMm,
        width_mm: f64,
    },
    Rectangle {
        start_mm: PointMm,
        end_mm: PointMm,
        width_mm: f64,
        filled: bool,
    },
    Circle {
        center_mm: PointMm,
        radius_mm: f64,
        width_mm: f64,
        filled: bool,
    },
    Arc {
        start_mm: PointMm,
        mid_mm: PointMm,
        end_mm: PointMm,
        width_mm: f64,
    },
    Polygon {
        points_mm: Vec<PointMm>,
        width_mm: f64,
        filled: bool,
    },
    Text {
        role: FootprintTextRole,
        text: String,
        position_mm: PointMm,
        rotation_degrees: f64,
        size_mm: SizeMm,
        thickness_mm: f64,
        font_face: Option<String>,
        bold: bool,
        italic: bool,
        mirrored: bool,
        hidden: bool,
        line_spacing: Option<f64>,
    },
    /// Unsupported drawable geometry is kept in the feature report and source.
    Unsupported { source_kind: String },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintGraphic {
    pub layer: FootprintLayer,
    pub style: FootprintStrokeStyle,
    pub geometry: FootprintGraphicGeometry,
}

/// Physical defaults attached to the footprint as a whole.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct FootprintRules {
    pub solder_mask_margin_mm: Option<f64>,
    pub solder_paste_margin_mm: Option<f64>,
    pub solder_paste_ratio: Option<f64>,
    pub clearance_mm: Option<f64>,
    pub thermal_width_mm: Option<f64>,
    pub thermal_gap_mm: Option<f64>,
    pub zone_connect: Option<String>,
    pub attributes: Vec<String>,
    pub private_layers: Vec<FootprintLayer>,
}

/// Provenance fields known at import time. License is intentionally explicit
/// and stays unknown until supplied by the library owner.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintProvenance {
    pub source_format: String,
    pub source_path: Option<String>,
    pub library: Option<String>,
    pub format_version: Option<u32>,
    pub generator: Option<String>,
    pub license: Option<String>,
}

/// A feature that was not converted into the model's physical/vector types.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnsupportedFootprintFeature {
    pub location: String,
    pub feature: String,
    pub raw_expression: String,
    pub blocks_fabrication: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FootprintValidationIssue {
    pub location: String,
    pub message: String,
}

/// A local footprint asset, including its verbatim source file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FootprintAsset {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub tags: Vec<String>,
    pub pads: Vec<FootprintPad>,
    pub graphics: Vec<FootprintGraphic>,
    pub rules: FootprintRules,
    pub provenance: FootprintProvenance,
    pub unsupported_features: Vec<UnsupportedFootprintFeature>,
    /// Exact input text, including comments and whitespace.
    pub source_text: String,
}
