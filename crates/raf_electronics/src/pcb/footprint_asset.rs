//! Physical, serializable PCB footprint assets and a conservative KiCad importer.
//!
//! Geometry in this module is expressed in millimeters. Imported source text is
//! retained verbatim so unsupported KiCad data cannot disappear on a local
//! save/load cycle.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const ASSET_SCHEMA_VERSION: u32 = 1;
const MAX_FOOTPRINT_SOURCE_BYTES: usize = 8 * 1024 * 1024;
const MAX_STORED_ASSET_BYTES: usize = 32 * 1024 * 1024;
const MAX_LIBRARY_TOTAL_BYTES: usize = 128 * 1024 * 1024;
const MAX_LIBRARY_ASSETS: usize = 4096;
const MAX_PADS_PER_ASSET: usize = 8192;
const MAX_GRAPHICS_PER_ASSET: usize = 32768;
const MAX_SEXPR_NODES: usize = 100_000;
const MAX_NESTING_DEPTH: usize = 128;
const MAX_ASSET_ID_BYTES: usize = 128;

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
    fn is_positive_finite(self) -> bool {
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
    fn from_kicad_name(name: &str) -> Self {
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

impl FootprintPad {
    /// Returns the four corners of the pad's rotated rectangular envelope.
    /// Rounded, circular, and oval pads use the same conservative envelope.
    pub fn corners_mm(&self) -> [PointMm; 4] {
        let half_width = self.size_mm.width * 0.5;
        let half_height = self.size_mm.height * 0.5;
        [
            PointMm {
                x: -half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: half_height,
            },
            PointMm {
                x: -half_width,
                y: half_height,
            },
        ]
        .map(|point| self.to_footprint_coordinates(point))
    }

    /// Approximates the supported pad outline with a conservative polygon in
    /// footprint-local coordinates. Curved outlines use circumscribed chords:
    /// the maximum radial overrun is reported by `outline_tolerance_mm()`.
    /// Returns `None` for an unsupported KiCad pad shape.
    pub fn outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.outline_polygon_bound_mm(curve_segments, true)
    }

    /// Inscribed copper is used to prove connectivity; a conservative outer
    /// bound used for clearance must never manufacture a false connection.
    pub(crate) fn contact_outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.outline_polygon_bound_mm(curve_segments, false)
    }

    fn outline_polygon_bound_mm(&self, curve_segments: usize, outer: bool) -> Option<Vec<PointMm>> {
        let half_width = self.size_mm.width * 0.5;
        let half_height = self.size_mm.height * 0.5;
        let mut local = Vec::new();
        match &self.shape {
            FootprintPadShape::Rectangle => {
                local.extend([
                    PointMm {
                        x: -half_width,
                        y: -half_height,
                    },
                    PointMm {
                        x: half_width,
                        y: -half_height,
                    },
                    PointMm {
                        x: half_width,
                        y: half_height,
                    },
                    PointMm {
                        x: -half_width,
                        y: half_height,
                    },
                ]);
            }
            FootprintPadShape::Circle => {
                append_circle_points(&mut local, PointMm::ZERO, half_width, curve_segments, outer);
            }
            FootprintPadShape::Oval => {
                append_capsule_points(
                    &mut local,
                    self.size_mm.width,
                    self.size_mm.height,
                    curve_segments,
                    outer,
                );
            }
            FootprintPadShape::RoundRect { corner_ratio } => {
                let ratio = (*corner_ratio)?;
                if !ratio.is_finite() || !(0.0..=0.5).contains(&ratio) {
                    return None;
                }
                let radius = self.size_mm.width.min(self.size_mm.height) * ratio;
                append_round_rect_points(
                    &mut local,
                    self.size_mm.width,
                    self.size_mm.height,
                    radius,
                    curve_segments,
                    outer,
                );
            }
            FootprintPadShape::Unsupported { .. } => return None,
        }
        Some(
            local
                .into_iter()
                .map(|point| self.to_footprint_coordinates(point))
                .collect(),
        )
    }

    /// Largest radial overrun introduced by the conservative curve polygon, in
    /// millimeters. Rectangles and pads with invalid or absent shape settings
    /// return `0.0` here; call `validate()` before using such geometry.
    pub fn outline_tolerance_mm(&self, curve_segments: usize) -> f64 {
        let segments = curve_segments.clamp(8, 4096);
        let radius = self.size_mm.width.min(self.size_mm.height) * 0.5;
        match &self.shape {
            FootprintPadShape::Circle => {
                let half_step = std::f64::consts::PI / segments as f64;
                radius * (half_step.cos().recip() - 1.0)
            }
            FootprintPadShape::Oval => {
                let per_half = (segments / 2).max(4);
                let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
                radius * (half_step.cos().recip() - 1.0)
            }
            FootprintPadShape::RoundRect {
                corner_ratio: Some(ratio),
            } if ratio.is_finite() && (0.0..=0.5).contains(ratio) => {
                let per_corner = (segments / 4).max(2);
                if *ratio == 0.0 {
                    0.0
                } else {
                    let half_step =
                        std::f64::consts::PI / (4.0 * per_corner.saturating_sub(1) as f64);
                    self.size_mm.width.min(self.size_mm.height)
                        * *ratio
                        * (half_step.cos().recip() - 1.0)
                }
            }
            _ => 0.0,
        }
    }

    /// Returns the drill outline, offset and rotated with its pad.
    pub fn drill_outline_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.drill_outline_bound_mm(curve_segments, false)
    }

    pub(crate) fn contact_drill_polygon_mm(&self, curve_segments: usize) -> Option<Vec<PointMm>> {
        self.drill_outline_bound_mm(curve_segments, true)
    }

    fn drill_outline_bound_mm(&self, curve_segments: usize, outer: bool) -> Option<Vec<PointMm>> {
        let drill = self.drill.as_ref()?;
        let mut local = Vec::new();
        match &drill.shape {
            FootprintDrillShape::Circular { diameter_mm } => append_circle_points(
                &mut local,
                drill.offset_mm,
                *diameter_mm * 0.5,
                curve_segments,
                outer,
            ),
            FootprintDrillShape::Oval {
                width_mm,
                height_mm,
            } => {
                let mut outline = Vec::new();
                append_capsule_points(&mut outline, *width_mm, *height_mm, curve_segments, outer);
                local.extend(outline.into_iter().map(|point| PointMm {
                    x: point.x + drill.offset_mm.x,
                    y: point.y + drill.offset_mm.y,
                }));
            }
        }
        Some(
            local
                .into_iter()
                .map(|point| self.to_footprint_coordinates(point))
                .collect(),
        )
    }

    /// Maximum radial undercut from approximating a drill with an inscribed
    /// polygon, in millimeters. Copper calculations keep the hole slightly
    /// smaller so the modeled annular copper is conservative.
    pub fn drill_outline_tolerance_mm(&self, curve_segments: usize) -> f64 {
        let Some(drill) = &self.drill else {
            return 0.0;
        };
        let segments = curve_segments.clamp(8, 4096);
        match &drill.shape {
            FootprintDrillShape::Circular { diameter_mm } => {
                let radius = *diameter_mm * 0.5;
                radius * (1.0 - (std::f64::consts::PI / segments as f64).cos())
            }
            FootprintDrillShape::Oval {
                width_mm,
                height_mm,
            } => {
                let radius = (*width_mm).min(*height_mm) * 0.5;
                let per_half = (segments / 2).max(4);
                let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
                radius * (1.0 - half_step.cos())
            }
        }
    }

    fn to_footprint_coordinates(&self, point: PointMm) -> PointMm {
        let radians = self.rotation_degrees.to_radians();
        let (sin, cos) = radians.sin_cos();
        PointMm {
            x: self.position_mm.x + point.x * cos - point.y * sin,
            y: self.position_mm.y + point.x * sin + point.y * cos,
        }
    }
}

fn append_circle_points(
    points: &mut Vec<PointMm>,
    center: PointMm,
    radius: f64,
    segments: usize,
    conservative: bool,
) {
    let count = segments.clamp(8, 4096);
    let half_step = std::f64::consts::PI / count as f64;
    let polygon_radius = if conservative {
        radius / half_step.cos()
    } else {
        radius * half_step.cos()
    };
    points.reserve(count);
    for index in 0..count {
        let angle = std::f64::consts::TAU * index as f64 / count as f64;
        points.push(PointMm {
            x: center.x + polygon_radius * angle.cos(),
            y: center.y + polygon_radius * angle.sin(),
        });
    }
}

fn append_capsule_points(
    points: &mut Vec<PointMm>,
    width: f64,
    height: f64,
    segments: usize,
    conservative: bool,
) {
    let per_half = (segments.clamp(8, 4096) / 2).max(4);
    let radius = width.min(height) * 0.5;
    let half_step = std::f64::consts::PI / (2.0 * per_half as f64);
    let polygon_radius = if conservative {
        radius / half_step.cos()
    } else {
        radius * half_step.cos()
    };
    if width >= height {
        let center_x = (width - height) * 0.5;
        for index in 0..=per_half {
            let angle = -std::f64::consts::FRAC_PI_2
                + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: center_x + polygon_radius * angle.cos(),
                y: polygon_radius * angle.sin(),
            });
        }
        for index in 0..=per_half {
            let angle =
                std::f64::consts::FRAC_PI_2 + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: -center_x + polygon_radius * angle.cos(),
                y: polygon_radius * angle.sin(),
            });
        }
    } else {
        let center_y = (height - width) * 0.5;
        for index in 0..=per_half {
            let angle = 0.0 + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: polygon_radius * angle.cos(),
                y: center_y + polygon_radius * angle.sin(),
            });
        }
        for index in 0..=per_half {
            let angle =
                std::f64::consts::PI + std::f64::consts::PI * index as f64 / per_half as f64;
            points.push(PointMm {
                x: polygon_radius * angle.cos(),
                y: -center_y + polygon_radius * angle.sin(),
            });
        }
    }
}

fn append_round_rect_points(
    points: &mut Vec<PointMm>,
    width: f64,
    height: f64,
    radius: f64,
    segments: usize,
    outer: bool,
) {
    let half_width = width * 0.5;
    let half_height = height * 0.5;
    let true_radius = radius.min(half_width).min(half_height);
    let per_corner = (segments.clamp(8, 4096) / 4).max(2);
    let draw_radius = if true_radius > 0.0 && outer {
        true_radius / (std::f64::consts::PI / (4.0 * per_corner.saturating_sub(1) as f64)).cos()
    } else {
        true_radius
    };
    if true_radius <= 0.0 {
        points.extend([
            PointMm {
                x: -half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: -half_height,
            },
            PointMm {
                x: half_width,
                y: half_height,
            },
            PointMm {
                x: -half_width,
                y: half_height,
            },
        ]);
        return;
    }
    let corners = [
        (
            half_width - true_radius,
            -half_height + true_radius,
            -std::f64::consts::FRAC_PI_2,
        ),
        (half_width - true_radius, half_height - true_radius, 0.0),
        (
            -half_width + true_radius,
            half_height - true_radius,
            std::f64::consts::FRAC_PI_2,
        ),
        (
            -half_width + true_radius,
            -half_height + true_radius,
            std::f64::consts::PI,
        ),
    ];
    for (center_x, center_y, start_angle) in corners {
        for index in 0..per_corner {
            let angle =
                start_angle + std::f64::consts::FRAC_PI_2 * index as f64 / (per_corner - 1) as f64;
            points.push(PointMm {
                x: center_x + draw_radius * angle.cos(),
                y: center_y + draw_radius * angle.sin(),
            });
        }
    }
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

impl FootprintAsset {
    /// Checks the numeric and structural invariants required by this model.
    pub fn validate(&self) -> Vec<FootprintValidationIssue> {
        let mut issues = Vec::new();
        if self.schema_version != ASSET_SCHEMA_VERSION {
            issue(
                &mut issues,
                "schema_version",
                format!("unsupported footprint asset schema {}", self.schema_version),
            );
        }
        if self.name.trim().is_empty()
            || safe_asset_stem(&self.name).as_deref() != Some(self.id.as_str())
        {
            issue(
                &mut issues,
                "identity",
                "asset id must be the safe normalized stem of a non-empty name",
            );
        }
        if self.source_text.len() > MAX_FOOTPRINT_SOURCE_BYTES {
            issue(
                &mut issues,
                "source_text",
                "source text exceeds the supported size limit",
            );
        }
        if self.pads.len() > MAX_PADS_PER_ASSET {
            issue(
                &mut issues,
                "pads",
                format!("asset exceeds the {} pad limit", MAX_PADS_PER_ASSET),
            );
        } else {
            for (index, pad) in self.pads.iter().enumerate() {
                let location = format!("pads[{index}] ({})", pad.number);
                validate_pad(pad, &location, &mut issues);
            }
        }
        if self.graphics.len() > MAX_GRAPHICS_PER_ASSET {
            issue(
                &mut issues,
                "graphics",
                format!(
                    "asset exceeds the {} graphics limit",
                    MAX_GRAPHICS_PER_ASSET
                ),
            );
        } else {
            for (index, graphic) in self.graphics.iter().enumerate() {
                let location = format!("graphics[{index}]");
                validate_graphic(graphic, &location, &mut issues);
            }
        }
        validate_optional_finite(
            self.rules.solder_mask_margin_mm,
            "rules.solder_mask_margin_mm",
            &mut issues,
        );
        validate_optional_finite(
            self.rules.solder_paste_margin_mm,
            "rules.solder_paste_margin_mm",
            &mut issues,
        );
        validate_optional_ratio(
            self.rules.solder_paste_ratio,
            "rules.solder_paste_ratio",
            &mut issues,
        );
        validate_optional_nonnegative(self.rules.clearance_mm, "rules.clearance_mm", &mut issues);
        validate_optional_nonnegative(
            self.rules.thermal_width_mm,
            "rules.thermal_width_mm",
            &mut issues,
        );
        validate_optional_nonnegative(
            self.rules.thermal_gap_mm,
            "rules.thermal_gap_mm",
            &mut issues,
        );
        issues
    }

    /// Returns false when any geometry is invalid or any physical source data
    /// remains unsupported.
    pub fn is_fabrication_ready(&self) -> bool {
        self.validate().is_empty()
            && !self
                .unsupported_features
                .iter()
                .any(|feature| feature.blocks_fabrication)
            && !self.pads.iter().any(|pad| {
                matches!(
                    &pad.shape,
                    FootprintPadShape::RoundRect { corner_ratio: None } | FootprintPadShape::Unsupported{..}
                )
                || matches!(pad.technology,FootprintPadTechnology::Unsupported{..})
                || pad.layers.iter().any(|layer|matches!(layer,FootprintLayer::InnerCopper(_)|FootprintLayer::Other(_)))
            })
            // Free copper, mask, paste and footprint-defined board cutouts need
            // their own connectivity/rules contract. Never silently omit them.
            && !self.graphics.iter().any(|graphic|matches!(graphic.layer,
                FootprintLayer::FrontCopper|FootprintLayer::BackCopper|FootprintLayer::AllCopper|FootprintLayer::InnerCopper(_)
                |FootprintLayer::FrontMask|FootprintLayer::BackMask|FootprintLayer::AllMask
                |FootprintLayer::FrontPaste|FootprintLayer::BackPaste|FootprintLayer::AllPaste|FootprintLayer::BoardOutline))
    }

    /// Returns the axis-aligned bounds of all pads and captured graphics.
    pub fn bounds_mm(&self) -> Option<FootprintBoundsMm> {
        let mut bounds = BoundsAccumulator::default();
        for pad in &self.pads {
            for corner in pad.corners_mm() {
                bounds.include(corner);
            }
        }
        for graphic in &self.graphics {
            bounds.include_graphic(graphic);
        }
        bounds.finish()
    }

    /// Returns the width and height of `bounds_mm()`.
    pub fn dimensions_mm(&self) -> Option<SizeMm> {
        self.bounds_mm().map(FootprintBoundsMm::size_mm)
    }
}

#[derive(Default)]
struct BoundsAccumulator {
    min: Option<PointMm>,
    max: Option<PointMm>,
}

impl BoundsAccumulator {
    fn include(&mut self, point: PointMm) {
        if !point.is_finite() {
            return;
        }
        self.min = Some(match self.min {
            Some(min) => PointMm {
                x: min.x.min(point.x),
                y: min.y.min(point.y),
            },
            None => point,
        });
        self.max = Some(match self.max {
            Some(max) => PointMm {
                x: max.x.max(point.x),
                y: max.y.max(point.y),
            },
            None => point,
        });
    }

    fn include_with_radius(&mut self, point: PointMm, radius: f64) {
        if !radius.is_finite() || radius < 0.0 {
            return;
        }
        self.include(PointMm {
            x: point.x - radius,
            y: point.y - radius,
        });
        self.include(PointMm {
            x: point.x + radius,
            y: point.y + radius,
        });
    }

    fn include_graphic(&mut self, graphic: &FootprintGraphic) {
        let half_stroke = match &graphic.geometry {
            FootprintGraphicGeometry::Line { width_mm, .. }
            | FootprintGraphicGeometry::Rectangle { width_mm, .. }
            | FootprintGraphicGeometry::Circle { width_mm, .. }
            | FootprintGraphicGeometry::Arc { width_mm, .. }
            | FootprintGraphicGeometry::Polygon { width_mm, .. } => width_mm * 0.5,
            FootprintGraphicGeometry::Text { thickness_mm, .. } => thickness_mm * 0.5,
            FootprintGraphicGeometry::Unsupported { .. } => return,
        };
        match &graphic.geometry {
            FootprintGraphicGeometry::Line {
                start_mm, end_mm, ..
            }
            | FootprintGraphicGeometry::Rectangle {
                start_mm, end_mm, ..
            } => {
                self.include_with_radius(*start_mm, half_stroke);
                self.include_with_radius(*end_mm, half_stroke);
            }
            FootprintGraphicGeometry::Circle {
                center_mm,
                radius_mm,
                ..
            } => self.include_with_radius(*center_mm, radius_mm + half_stroke),
            FootprintGraphicGeometry::Arc {
                start_mm,
                mid_mm,
                end_mm,
                ..
            } => {
                self.include_with_radius(*start_mm, half_stroke);
                self.include_with_radius(*mid_mm, half_stroke);
                self.include_with_radius(*end_mm, half_stroke);
            }
            FootprintGraphicGeometry::Polygon { points_mm, .. } => {
                for point in points_mm {
                    self.include_with_radius(*point, half_stroke);
                }
            }
            FootprintGraphicGeometry::Text {
                position_mm,
                rotation_degrees,
                size_mm,
                ..
            } => {
                let radians = rotation_degrees.to_radians();
                let (sin, cos) = radians.sin_cos();
                let half_width = size_mm.width * 0.5 + half_stroke;
                let half_height = size_mm.height * 0.5 + half_stroke;
                for (x, y) in [
                    (-half_width, -half_height),
                    (half_width, -half_height),
                    (half_width, half_height),
                    (-half_width, half_height),
                ] {
                    self.include(PointMm {
                        x: position_mm.x + x * cos - y * sin,
                        y: position_mm.y + x * sin + y * cos,
                    });
                }
            }
            FootprintGraphicGeometry::Unsupported { .. } => {}
        }
    }

    fn finish(self) -> Option<FootprintBoundsMm> {
        Some(FootprintBoundsMm {
            min: self.min?,
            max: self.max?,
        })
    }
}

fn validate_pad(pad: &FootprintPad, location: &str, issues: &mut Vec<FootprintValidationIssue>) {
    if pad.number.trim().is_empty()
        && !matches!(
            &pad.technology,
            FootprintPadTechnology::NonPlatedThroughHole
        )
    {
        issue(issues, format!("{location}.number"), "pad number is empty");
    }
    if !pad.position_mm.is_finite() {
        issue(
            issues,
            format!("{location}.position_mm"),
            "coordinates must be finite",
        );
    }
    if !pad.rotation_degrees.is_finite() {
        issue(
            issues,
            format!("{location}.rotation_degrees"),
            "rotation must be finite",
        );
    }
    if !pad.size_mm.is_positive_finite() {
        issue(
            issues,
            format!("{location}.size_mm"),
            "width and height must be finite and greater than zero",
        );
    }
    if pad.layers.is_empty() {
        issue(
            issues,
            format!("{location}.layers"),
            "at least one layer is required",
        );
    }
    if matches!(&pad.shape, FootprintPadShape::Circle)
        && pad.size_mm.width.is_finite()
        && pad.size_mm.height.is_finite()
        && (pad.size_mm.width - pad.size_mm.height).abs() > 1e-6
    {
        issue(
            issues,
            format!("{location}.size_mm"),
            "a circular pad must have equal width and height",
        );
    }
    match &pad.shape {
        FootprintPadShape::RoundRect {
            corner_ratio: Some(value),
        } if !value.is_finite() || !(0.0..=0.5).contains(value) => issue(
            issues,
            format!("{location}.shape.corner_ratio"),
            "corner ratio must be finite and between zero and 0.5",
        ),
        FootprintPadShape::RoundRect { corner_ratio: None } => {}
        FootprintPadShape::RoundRect {
            corner_ratio: Some(_),
        } => {}
        _ => {}
    }
    validate_optional_finite(
        pad.solder_mask_margin_mm,
        &format!("{location}.solder_mask_margin_mm"),
        issues,
    );
    validate_optional_finite(
        pad.solder_paste_margin_mm,
        &format!("{location}.solder_paste_margin_mm"),
        issues,
    );
    validate_optional_ratio(
        pad.solder_paste_margin_ratio,
        &format!("{location}.solder_paste_margin_ratio"),
        issues,
    );

    let has_front_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::FrontCopper | FootprintLayer::AllCopper
        )
    });
    let has_back_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::BackCopper | FootprintLayer::AllCopper
        )
    });
    let has_any_copper = pad.layers.iter().any(|layer| {
        matches!(
            layer,
            FootprintLayer::FrontCopper
                | FootprintLayer::BackCopper
                | FootprintLayer::InnerCopper(_)
                | FootprintLayer::AllCopper
        )
    });
    if matches!(&pad.technology, FootprintPadTechnology::Smd) && !has_any_copper {
        issue(
            issues,
            format!("{location}.layers"),
            "an SMD pad must include a copper layer",
        );
    }
    if matches!(&pad.technology, FootprintPadTechnology::PlatedThroughHole)
        && !(has_front_copper && has_back_copper)
    {
        issue(
            issues,
            format!("{location}.layers"),
            "a plated through-hole pad must include front and back copper",
        );
    }
    // KiCad NPTH pads commonly declare *.Cu to identify the pierced layers.
    // Their technology excludes them from copper; this is not an annulus.
    match (&pad.technology, &pad.drill) {
        (FootprintPadTechnology::Smd, Some(_)) => issue(
            issues,
            format!("{location}.drill"),
            "an SMD pad cannot define a drill",
        ),
        (FootprintPadTechnology::PlatedThroughHole, None) => issue(
            issues,
            format!("{location}.drill"),
            "a plated through-hole pad requires a drill",
        ),
        (FootprintPadTechnology::NonPlatedThroughHole, None) => issue(
            issues,
            format!("{location}.drill"),
            "a non-plated through-hole pad requires a drill",
        ),
        (_, Some(drill)) => validate_drill(drill, &format!("{location}.drill"), issues),
        _ => {}
    }
    if matches!(pad.technology, FootprintPadTechnology::PlatedThroughHole)
        && pad.size_mm.is_positive_finite()
        && pad.rotation_degrees.is_finite()
    {
        if let (Some(mut copper), Some(drill)) = (
            pad.contact_outline_polygon_mm(128),
            pad.contact_drill_polygon_mm(128),
        ) {
            if let Some(first) = copper.first().copied() {
                copper.push(first);
            }
            let copper = copper
                .iter()
                .map(|p| glam::Vec2::new(p.x as f32, p.y as f32))
                .collect::<Vec<_>>();
            if drill.iter().any(|p| {
                !super::geometry::point_in_polygon(glam::Vec2::new(p.x as f32, p.y as f32), &copper)
            }) {
                issue(
                    issues,
                    format!("{location}.drill"),
                    "plated drill must fit completely inside its copper pad",
                );
            }
        }
    }
}

fn validate_drill(
    drill: &FootprintDrill,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if !drill.offset_mm.is_finite() {
        issue(
            issues,
            format!("{location}.offset_mm"),
            "drill offset coordinates must be finite",
        );
    }
    match &drill.shape {
        FootprintDrillShape::Circular { diameter_mm }
            if !diameter_mm.is_finite() || *diameter_mm <= 0.0 =>
        {
            issue(
                issues,
                format!("{location}.diameter_mm"),
                "drill diameter must be finite and greater than zero",
            );
        }
        FootprintDrillShape::Oval {
            width_mm,
            height_mm,
        } if !(SizeMm {
            width: *width_mm,
            height: *height_mm,
        })
        .is_positive_finite() =>
        {
            issue(
                issues,
                format!("{location}.size_mm"),
                "oval drill width and height must be finite and greater than zero",
            )
        }
        _ => {}
    }
}

fn validate_graphic(
    graphic: &FootprintGraphic,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    match &graphic.geometry {
        FootprintGraphicGeometry::Line {
            start_mm,
            end_mm,
            width_mm,
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Rectangle {
            start_mm,
            end_mm,
            width_mm,
            ..
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Circle {
            center_mm,
            radius_mm,
            width_mm,
            ..
        } => {
            validate_graphic_point(*center_mm, location, "center_mm", issues);
            validate_positive(*radius_mm, &format!("{location}.radius_mm"), issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Arc {
            start_mm,
            mid_mm,
            end_mm,
            width_mm,
        } => {
            validate_graphic_point(*start_mm, location, "start_mm", issues);
            validate_graphic_point(*mid_mm, location, "mid_mm", issues);
            validate_graphic_point(*end_mm, location, "end_mm", issues);
            validate_positive(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Polygon {
            points_mm,
            width_mm,
            ..
        } => {
            if points_mm.len() < 3 {
                issue(
                    issues,
                    format!("{location}.points_mm"),
                    "a polygon requires at least three points",
                );
            }
            for (index, point) in points_mm.iter().enumerate() {
                validate_graphic_point(*point, location, &format!("points_mm[{index}]"), issues);
            }
            validate_nonnegative(*width_mm, &format!("{location}.width_mm"), issues);
        }
        FootprintGraphicGeometry::Text {
            position_mm,
            rotation_degrees,
            size_mm,
            thickness_mm,
            line_spacing,
            ..
        } => {
            validate_graphic_point(*position_mm, location, "position_mm", issues);
            if !rotation_degrees.is_finite() {
                issue(
                    issues,
                    format!("{location}.rotation_degrees"),
                    "rotation must be finite",
                );
            }
            if !size_mm.is_positive_finite() {
                issue(
                    issues,
                    format!("{location}.size_mm"),
                    "text width and height must be finite and greater than zero",
                );
            }
            validate_positive(*thickness_mm, &format!("{location}.thickness_mm"), issues);
            if let Some(value) = line_spacing {
                validate_positive(*value, &format!("{location}.line_spacing"), issues);
            }
        }
        FootprintGraphicGeometry::Unsupported { .. } => {}
    }
    if let FootprintStrokeStyle::Unsupported(style) = &graphic.style {
        issue(
            issues,
            format!("{location}.style"),
            format!("unsupported stroke style '{style}'"),
        );
    }
}

fn validate_graphic_point(
    point: PointMm,
    location: &str,
    field: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if !point.is_finite() {
        issue(
            issues,
            format!("{location}.{field}"),
            "coordinates must be finite",
        );
    }
}

fn validate_optional_nonnegative(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if let Some(value) = value {
        validate_nonnegative(value, location, issues);
    }
}

fn validate_optional_ratio(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if let Some(value) = value {
        if !value.is_finite() || !(-1.0..=1.0).contains(&value) {
            issue(
                issues,
                location,
                "paste ratio must be finite and between -1 and 1",
            );
        }
    }
}

fn validate_optional_finite(
    value: Option<f64>,
    location: &str,
    issues: &mut Vec<FootprintValidationIssue>,
) {
    if value.is_some_and(|v| !v.is_finite()) {
        issue(issues, location, "margin must be finite");
    }
}

fn validate_positive(value: f64, location: &str, issues: &mut Vec<FootprintValidationIssue>) {
    if !value.is_finite() || value <= 0.0 {
        issue(
            issues,
            location,
            "value must be finite and greater than zero",
        );
    }
}

fn validate_nonnegative(value: f64, location: &str, issues: &mut Vec<FootprintValidationIssue>) {
    if !value.is_finite() || value < 0.0 {
        issue(issues, location, "value must be finite and non-negative");
    }
}

fn issue(
    issues: &mut Vec<FootprintValidationIssue>,
    location: impl Into<String>,
    message: impl Into<String>,
) {
    issues.push(FootprintValidationIssue {
        location: location.into(),
        message: message.into(),
    });
}

/// Syntax or data error reported while reading one KiCad footprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FootprintImportError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl FootprintImportError {
    fn at(source: &str, offset: usize, message: impl Into<String>) -> Self {
        let safe_offset = offset.min(source.len());
        let prefix = &source[..safe_offset];
        let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
        let column = prefix
            .rsplit('\n')
            .next()
            .map(|tail| tail.chars().count() + 1)
            .unwrap_or(1);
        Self {
            message: message.into(),
            line: Some(line),
            column: Some(column),
        }
    }

    fn data(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }
}

impl fmt::Display for FootprintImportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let (Some(line), Some(column)) = (self.line, self.column) {
            write!(
                formatter,
                "{} (línea {line}, columna {column})",
                self.message
            )
        } else {
            formatter.write_str(&self.message)
        }
    }
}

impl Error for FootprintImportError {}

/// Errors while reading or writing the local footprint collection.
#[derive(Debug)]
pub enum FootprintLibraryError {
    Io(io::Error),
    Parse(FootprintImportError),
    Serialization(String),
    InvalidName(String),
    DuplicateName(String),
    CorruptAsset { path: PathBuf, message: String },
    ResourceLimit { path: PathBuf, limit_bytes: usize },
    TooManyAssets { directory: PathBuf, limit: usize },
}

impl fmt::Display for FootprintLibraryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "footprint library I/O error: {error}"),
            Self::Parse(error) => write!(formatter, "could not import footprint: {error}"),
            Self::Serialization(error) => {
                write!(formatter, "could not serialize footprint: {error}")
            }
            Self::InvalidName(name) => write!(formatter, "invalid footprint name '{name}'"),
            Self::DuplicateName(name) => write!(formatter, "footprint '{name}' already exists"),
            Self::CorruptAsset { path, message } => {
                write!(
                    formatter,
                    "invalid saved footprint '{}': {message}",
                    path.display()
                )
            }
            Self::ResourceLimit { path, limit_bytes } => write!(
                formatter,
                "footprint data '{}' exceeds the {limit_bytes}-byte resource limit",
                path.display()
            ),
            Self::TooManyAssets { directory, limit } => write!(
                formatter,
                "footprint library '{}' exceeds the {limit}-asset limit",
                directory.display()
            ),
        }
    }
}

impl Error for FootprintLibraryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Parse(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for FootprintLibraryError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FootprintImportError> for FootprintLibraryError {
    fn from(value: FootprintImportError) -> Self {
        Self::Parse(value)
    }
}

/// Parses a single `.kicad_mod` file. Unsupported physical features stay in
/// `unsupported_features` and make `is_fabrication_ready()` return false.
pub fn parse_kicad_footprint(source: &str) -> Result<FootprintAsset, FootprintImportError> {
    if source.len() > MAX_FOOTPRINT_SOURCE_BYTES {
        return Err(FootprintImportError::data(format!(
            "footprint source exceeds the {MAX_FOOTPRINT_SOURCE_BYTES}-byte size limit"
        )));
    }
    let root = SExprParser::new(source).parse_document()?;
    parse_footprint_node(source, &root)
}

fn parse_footprint_node(
    source: &str,
    root: &SExpr,
) -> Result<FootprintAsset, FootprintImportError> {
    let Some(items) = root.as_list() else {
        return Err(FootprintImportError::at(
            source,
            root.start,
            "top-level expression must be a footprint list",
        ));
    };
    let root_kind = items.first().and_then(SExpr::as_text).unwrap_or_default();
    if root_kind != "footprint" && root_kind != "module" {
        return Err(FootprintImportError::at(
            source,
            root.start,
            format!("expected (footprint ...), found ({root_kind} ...)"),
        ));
    }
    let name = items
        .get(1)
        .and_then(SExpr::as_text)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            FootprintImportError::at(source, root.start, "footprint name is missing or empty")
        })?
        .to_string();

    let mut asset = FootprintAsset {
        schema_version: ASSET_SCHEMA_VERSION,
        id: safe_asset_stem(&name).ok_or_else(|| {
            FootprintImportError::at(
                source,
                items[1].start,
                "footprint name cannot form a local asset id",
            )
        })?,
        name,
        description: None,
        tags: Vec::new(),
        pads: Vec::new(),
        graphics: Vec::new(),
        rules: FootprintRules::default(),
        provenance: FootprintProvenance {
            source_format: if root_kind == "module" {
                "kicad_mod_legacy_module".to_string()
            } else {
                "kicad_mod".to_string()
            },
            ..FootprintProvenance::default()
        },
        unsupported_features: Vec::new(),
        source_text: source.to_string(),
    };

    for item in items.iter().skip(2) {
        let Some(child) = item.as_list() else {
            if let Some(attribute) = item.as_text() {
                if attribute == "locked" || attribute == "placed" {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        attribute,
                        item,
                        false,
                    ));
                } else {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        attribute,
                        item,
                        true,
                    ));
                }
            }
            continue;
        };
        let Some(kind) = child.first().and_then(SExpr::as_text) else {
            continue;
        };
        match kind {
            "version" => {
                asset.provenance.format_version = child
                    .get(1)
                    .and_then(SExpr::as_text)
                    .and_then(|value| value.parse::<u32>().ok());
            }
            "generator" => {
                asset.provenance.generator =
                    child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "descr" => {
                asset.description = child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "tags" => {
                if let Some(tags) = child.get(1).and_then(SExpr::as_text) {
                    asset.tags = split_tags(tags);
                }
            }
            "property" => {
                let property = child.get(1).and_then(SExpr::as_text).unwrap_or_default();
                if matches!(
                    property.to_ascii_lowercase().as_str(),
                    "license" | "licence"
                ) {
                    asset.provenance.license =
                        child.get(2).and_then(SExpr::as_text).map(str::to_owned);
                } else {
                    asset.unsupported_features.push(feature(
                        source,
                        "footprint",
                        "property metadata",
                        item,
                        false,
                    ));
                }
            }
            "solder_mask_margin" => {
                asset.rules.solder_mask_margin_mm =
                    Some(number_required(source, child.get(1), "solder_mask_margin")?);
            }
            "solder_paste_margin" => {
                asset.rules.solder_paste_margin_mm = Some(number_required(
                    source,
                    child.get(1),
                    "solder_paste_margin",
                )?);
            }
            "solder_paste_ratio" => {
                asset.rules.solder_paste_ratio =
                    Some(number_required(source, child.get(1), "solder_paste_ratio")?);
            }
            "clearance" => {
                asset.rules.clearance_mm =
                    Some(number_required(source, child.get(1), "clearance")?);
            }
            "thermal_width" => {
                asset.rules.thermal_width_mm =
                    Some(number_required(source, child.get(1), "thermal_width")?);
            }
            "thermal_gap" => {
                asset.rules.thermal_gap_mm =
                    Some(number_required(source, child.get(1), "thermal_gap")?);
            }
            "zone_connect" => {
                asset.rules.zone_connect = child.get(1).and_then(SExpr::as_text).map(str::to_owned);
            }
            "attr" => {
                asset.rules.attributes = child
                    .iter()
                    .skip(1)
                    .filter_map(SExpr::as_text)
                    .map(str::to_owned)
                    .collect();
            }
            "private_layers" => {
                asset.rules.private_layers = child
                    .iter()
                    .skip(1)
                    .filter_map(SExpr::as_text)
                    .map(FootprintLayer::from_kicad_name)
                    .collect();
            }
            "pad" => parse_pad(source, item, &mut asset)?,
            "fp_line" | "fp_rect" | "fp_circle" | "fp_arc" | "fp_poly" | "fp_text" => {
                parse_graphic(source, item, &mut asset)?;
            }
            "layer" | "tedit" | "tstamp" | "uuid" | "path" | "autoplace_cost90"
            | "autoplace_cost180" | "locked" | "placed" => {
                asset.unsupported_features.push(feature(
                    source,
                    "footprint",
                    format!("non-geometry metadata '{kind}'"),
                    item,
                    false,
                ));
            }
            "model" | "image" => asset.unsupported_features.push(feature(
                source,
                "footprint",
                format!("presentation data '{kind}'"),
                item,
                false,
            )),
            _ => asset.unsupported_features.push(feature(
                source,
                "footprint",
                format!("unhandled footprint feature '{kind}'"),
                item,
                true,
            )),
        }
    }

    for (index, pad) in asset.pads.iter().enumerate() {
        let location = format!("pad[{index}] ({})", pad.number);
        let mut issues = Vec::new();
        validate_pad(pad, &location, &mut issues);
        if let Some(issue) = issues.first() {
            return Err(FootprintImportError::data(format!(
                "{}: {}",
                issue.location, issue.message
            )));
        }
    }
    for (index, graphic) in asset.graphics.iter().enumerate() {
        let location = format!("graphics[{index}]");
        let mut issues = Vec::new();
        validate_graphic(graphic, &location, &mut issues);
        if let Some(issue) = issues.first() {
            return Err(FootprintImportError::data(format!(
                "{}: {}",
                issue.location, issue.message
            )));
        }
    }
    if let Some(issue) = asset.validate().first() {
        return Err(FootprintImportError::data(format!(
            "{}: {}",
            issue.location, issue.message
        )));
    }
    Ok(asset)
}

fn parse_pad(
    source: &str,
    expression: &SExpr,
    asset: &mut FootprintAsset,
) -> Result<(), FootprintImportError> {
    let Some(items) = expression.as_list() else {
        return Ok(());
    };
    if items.len() < 6 {
        return Err(FootprintImportError::at(
            source,
            expression.start,
            "pad needs a number, type, shape, position, and size",
        ));
    }
    let number_text = items.get(1).and_then(SExpr::as_text).unwrap_or_default();
    let source_type = items.get(2).and_then(SExpr::as_text).unwrap_or_default();
    let source_shape = items.get(3).and_then(SExpr::as_text).unwrap_or_default();
    let at = find_child(items, "at").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no at field"),
        )
    })?;
    let (position_mm, rotation_degrees) = parse_position(source, at, "pad at")?;
    let size = find_child(items, "size").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no size field"),
        )
    })?;
    let size_items = size.as_list().unwrap_or_default();
    let size_mm = SizeMm {
        width: number_required(source, size_items.get(1), "pad size width")?,
        height: number_required(source, size_items.get(2), "pad size height")?,
    };

    let (technology, technology_supported) = match source_type {
        "smd" => (FootprintPadTechnology::Smd, true),
        "thru_hole" => (FootprintPadTechnology::PlatedThroughHole, true),
        "np_thru_hole" => (FootprintPadTechnology::NonPlatedThroughHole, true),
        other => (
            FootprintPadTechnology::Unsupported {
                source_type: other.to_string(),
            },
            false,
        ),
    };
    if !technology_supported {
        asset.unsupported_features.push(feature(
            source,
            format!("pad '{number_text}'"),
            format!("pad type '{source_type}'"),
            expression,
            true,
        ));
    }

    let ratio = find_child(items, "roundrect_rratio")
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "roundrect_rratio"))
        .transpose()?;
    let shape = match source_shape {
        "rect" => FootprintPadShape::Rectangle,
        "circle" => FootprintPadShape::Circle,
        "oval" => FootprintPadShape::Oval,
        "roundrect" => FootprintPadShape::RoundRect {
            corner_ratio: ratio,
        },
        other => {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("pad shape '{other}'"),
                expression,
                true,
            ));
            FootprintPadShape::Unsupported {
                source_shape: other.to_string(),
            }
        }
    };

    let layer_node = find_child(items, "layers").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("pad '{number_text}' has no layers field"),
        )
    })?;
    let layer_items = layer_node.as_list().unwrap_or_default();
    let mut layers = Vec::new();
    for item in layer_items.iter().skip(1) {
        let Some(name) = item.as_text() else {
            return Err(FootprintImportError::at(
                source,
                item.start,
                format!("pad '{number_text}' has a non-string layer"),
            ));
        };
        let layer = FootprintLayer::from_kicad_name(name);
        if matches!(&layer, FootprintLayer::Other(_)) {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("pad layer '{name}'"),
                layer_node,
                true,
            ));
        }
        if matches!(&layer, FootprintLayer::InnerCopper(_)) {
            asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("inner copper layer '{name}' is outside the two-layer PCB stack"),
                layer_node,
                true,
            ));
        }
        layers.push(layer);
    }

    let drill = find_child(items, "drill")
        .map(|node| parse_drill(source, node))
        .transpose()?;
    let solder_mask_margin_mm = parse_optional_number(source, items, "solder_mask_margin")?;
    let solder_paste_margin_mm = parse_optional_number(source, items, "solder_paste_margin")?;
    let solder_paste_margin_ratio =
        parse_optional_number(source, items, "solder_paste_margin_ratio")?;
    let pad = FootprintPad {
        number: number_text.to_string(),
        position_mm,
        rotation_degrees,
        size_mm,
        shape,
        technology,
        layers,
        drill,
        solder_mask_margin_mm,
        solder_paste_margin_mm,
        solder_paste_margin_ratio,
    };

    for child in items
        .iter()
        .skip(4)
        .filter(|child| child.as_list().is_some())
    {
        let children = child.as_list().unwrap_or_default();
        let kind = children
            .first()
            .and_then(SExpr::as_text)
            .unwrap_or_default();
        match kind {
            "at"
            | "size"
            | "drill"
            | "layers"
            | "roundrect_rratio"
            | "solder_mask_margin"
            | "solder_paste_margin"
            | "solder_paste_margin_ratio"
            | "net"
            | "uuid"
            | "pinfunction"
            | "pintype"
            | "property"
            | "clearance"
            | "zone_connect"
            | "thermal_width"
            | "thermal_gap"
            | "die_length" => {}
            "chamfer" | "chamfer_ratio" => asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("chamfered pad feature '{kind}'"),
                child,
                true,
            )),
            _ => asset.unsupported_features.push(feature(
                source,
                format!("pad '{number_text}'"),
                format!("unhandled pad feature '{kind}'"),
                child,
                true,
            )),
        }
    }
    if matches!(source_shape, "roundrect") && ratio.is_none() {
        asset.unsupported_features.push(feature(
            source,
            format!("pad '{number_text}'"),
            "round rectangle with implicit corner ratio",
            expression,
            true,
        ));
    }
    asset.pads.push(pad);
    Ok(())
}

fn parse_drill(source: &str, expression: &SExpr) -> Result<FootprintDrill, FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    let is_oval = values.get(1).and_then(SExpr::as_text) == Some("oval");
    let first_number_index = if is_oval { 2 } else { 1 };
    let first = number_required(source, values.get(first_number_index), "drill diameter")?;
    let shape = if is_oval {
        let second = number_required(
            source,
            values.get(first_number_index + 1),
            "oval drill height",
        )?;
        FootprintDrillShape::Oval {
            width_mm: first,
            height_mm: second,
        }
    } else {
        FootprintDrillShape::Circular { diameter_mm: first }
    };
    let offset_mm = find_child(values, "offset")
        .map(|node| parse_point(source, node, "drill offset"))
        .transpose()?
        .unwrap_or(PointMm::ZERO);
    Ok(FootprintDrill { shape, offset_mm })
}

fn parse_graphic(
    source: &str,
    expression: &SExpr,
    asset: &mut FootprintAsset,
) -> Result<(), FootprintImportError> {
    let Some(items) = expression.as_list() else {
        return Ok(());
    };
    let kind = items.first().and_then(SExpr::as_text).unwrap_or_default();
    let layer_node = find_child(items, "layer").ok_or_else(|| {
        FootprintImportError::at(
            source,
            expression.start,
            format!("graphic '{kind}' has no layer field"),
        )
    })?;
    let layer_name = layer_node
        .as_list()
        .and_then(|layer| layer.get(1))
        .and_then(SExpr::as_text)
        .ok_or_else(|| {
            FootprintImportError::at(source, layer_node.start, "graphic layer is malformed")
        })?;
    let layer = FootprintLayer::from_kicad_name(layer_name);
    let style = parse_stroke_style(items);
    if matches!(&style, FootprintStrokeStyle::Unsupported(_)) {
        asset.unsupported_features.push(feature(
            source,
            format!("graphic on {layer_name}"),
            "unsupported stroke style",
            expression,
            true,
        ));
    }
    let width = if kind == "fp_text" {
        0.0
    } else {
        parse_graphic_width(source, items)?
    };
    let geometry = match kind {
        "fp_line" => FootprintGraphicGeometry::Line {
            start_mm: required_point(source, items, "start")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
        },
        "fp_rect" => FootprintGraphicGeometry::Rectangle {
            start_mm: required_point(source, items, "start")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
            filled: parse_filled(items),
        },
        "fp_circle" => {
            let center_mm = required_point(source, items, "center")?;
            let end_mm = required_point(source, items, "end")?;
            let dx = end_mm.x - center_mm.x;
            let dy = end_mm.y - center_mm.y;
            FootprintGraphicGeometry::Circle {
                center_mm,
                radius_mm: dx.hypot(dy),
                width_mm: width,
                filled: parse_filled(items),
            }
        }
        "fp_arc" => FootprintGraphicGeometry::Arc {
            start_mm: required_point(source, items, "start")?,
            mid_mm: required_point(source, items, "mid")?,
            end_mm: required_point(source, items, "end")?,
            width_mm: width,
        },
        "fp_poly" => FootprintGraphicGeometry::Polygon {
            points_mm: parse_points(source, items)?,
            width_mm: width,
            filled: parse_filled(items),
        },
        "fp_text" => parse_text(source, items)?,
        _ => FootprintGraphicGeometry::Unsupported {
            source_kind: kind.to_string(),
        },
    };
    if matches!(&layer, FootprintLayer::Other(_)) {
        asset.unsupported_features.push(feature(
            source,
            format!("graphic '{kind}'"),
            format!("graphic layer '{layer_name}'"),
            layer_node,
            layer.is_physical(),
        ));
    }
    asset.graphics.push(FootprintGraphic {
        layer,
        style,
        geometry,
    });
    Ok(())
}

fn parse_text(
    source: &str,
    items: &[SExpr],
) -> Result<FootprintGraphicGeometry, FootprintImportError> {
    let role_text = items.get(1).and_then(SExpr::as_text).unwrap_or_default();
    let text = items.get(2).and_then(SExpr::as_text).ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "fp_text string is missing")
    })?;
    let at = find_child(items, "at").ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "fp_text has no at field")
    })?;
    let (position_mm, rotation_degrees) = parse_position(source, at, "fp_text at")?;
    let effects = find_child(items, "effects");
    let (size_mm, thickness_mm, font_face, bold, italic, mirrored, line_spacing) =
        parse_text_effects(source, effects)?;
    Ok(FootprintGraphicGeometry::Text {
        role: match role_text {
            "reference" => FootprintTextRole::Reference,
            "value" => FootprintTextRole::Value,
            "user" => FootprintTextRole::User,
            other => FootprintTextRole::Other(other.to_string()),
        },
        text: text.to_string(),
        position_mm,
        rotation_degrees,
        size_mm,
        thickness_mm,
        font_face,
        bold,
        italic,
        mirrored,
        hidden: items.iter().any(|item| item.as_text() == Some("hide")),
        line_spacing,
    })
}

type TextEffects = (SizeMm, f64, Option<String>, bool, bool, bool, Option<f64>);

fn parse_text_effects(
    source: &str,
    effects: Option<&SExpr>,
) -> Result<TextEffects, FootprintImportError> {
    let Some(effects) = effects.and_then(SExpr::as_list) else {
        return Ok((
            SizeMm {
                width: 1.0,
                height: 1.0,
            },
            0.15,
            None,
            false,
            false,
            false,
            None,
        ));
    };
    let font = find_child(effects, "font").and_then(SExpr::as_list);
    let size_node = font.and_then(|font| find_child(font, "size"));
    let size_mm = if let Some(size_node) = size_node {
        let size = size_node.as_list().unwrap_or_default();
        SizeMm {
            height: number_required(source, size.get(1), "text height")?,
            width: number_required(source, size.get(2), "text width")?,
        }
    } else {
        SizeMm {
            width: 1.0,
            height: 1.0,
        }
    };
    let thickness_mm = font
        .and_then(|font| find_child(font, "thickness"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "text thickness"))
        .transpose()?
        .unwrap_or(0.15);
    let font_face = font
        .and_then(|font| find_child(font, "face"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
        .map(str::to_owned);
    let bold = font.is_some_and(|font| font.iter().any(|item| item.as_text() == Some("bold")));
    let italic = font.is_some_and(|font| font.iter().any(|item| item.as_text() == Some("italic")));
    let justify = find_child(effects, "justify").and_then(SExpr::as_list);
    let mirrored =
        justify.is_some_and(|items| items.iter().any(|item| item.as_text() == Some("mirror")));
    let line_spacing = font
        .and_then(|font| find_child(font, "line_spacing"))
        .and_then(|node| node.as_list())
        .and_then(|values| values.get(1))
        .map(|value| number_required(source, Some(value), "text line spacing"))
        .transpose()?;
    Ok((
        size_mm,
        thickness_mm,
        font_face,
        bold,
        italic,
        mirrored,
        line_spacing,
    ))
}

fn parse_graphic_width(source: &str, items: &[SExpr]) -> Result<f64, FootprintImportError> {
    if let Some(width) = find_child(items, "width") {
        return number_required(
            source,
            width.as_list().and_then(|values| values.get(1)),
            "graphic width",
        );
    }
    if let Some(stroke) = find_child(items, "stroke") {
        return number_required(
            source,
            find_child(stroke.as_list().unwrap_or_default(), "width")
                .and_then(|node| node.as_list())
                .and_then(|values| values.get(1)),
            "graphic stroke width",
        );
    }
    Err(FootprintImportError::at(
        source,
        items[0].start,
        "graphic has no width or stroke width",
    ))
}

fn parse_stroke_style(items: &[SExpr]) -> FootprintStrokeStyle {
    let Some(stroke) = find_child(items, "stroke").and_then(SExpr::as_list) else {
        return FootprintStrokeStyle::Default;
    };
    let Some(style) = find_child(stroke, "type")
        .and_then(SExpr::as_list)
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
    else {
        return FootprintStrokeStyle::Default;
    };
    match style {
        "default" => FootprintStrokeStyle::Default,
        "solid" => FootprintStrokeStyle::Solid,
        "dash" => FootprintStrokeStyle::Dashed,
        "dot" => FootprintStrokeStyle::Dotted,
        "dash_dot" => FootprintStrokeStyle::DashDot,
        "dash_dot_dot" => FootprintStrokeStyle::DashDotDot,
        other => FootprintStrokeStyle::Unsupported(other.to_string()),
    }
}

fn parse_filled(items: &[SExpr]) -> bool {
    find_child(items, "fill")
        .and_then(SExpr::as_list)
        .and_then(|values| values.get(1))
        .and_then(SExpr::as_text)
        .is_some_and(|value| matches!(value, "yes" | "solid"))
}

fn parse_points(source: &str, items: &[SExpr]) -> Result<Vec<PointMm>, FootprintImportError> {
    let points = find_child(items, "pts").ok_or_else(|| {
        FootprintImportError::at(source, items[0].start, "polygon has no pts field")
    })?;
    points
        .as_list()
        .unwrap_or_default()
        .iter()
        .skip(1)
        .map(|point| parse_point(source, point, "polygon point"))
        .collect()
}

fn required_point(
    source: &str,
    items: &[SExpr],
    name: &str,
) -> Result<PointMm, FootprintImportError> {
    let node = find_child(items, name).ok_or_else(|| {
        FootprintImportError::at(
            source,
            items[0].start,
            format!("graphic has no {name} field"),
        )
    })?;
    parse_point(source, node, name)
}

fn parse_position(
    source: &str,
    expression: &SExpr,
    label: &str,
) -> Result<(PointMm, f64), FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    let point = PointMm {
        x: number_required(source, values.get(1), &format!("{label} x"))?,
        y: number_required(source, values.get(2), &format!("{label} y"))?,
    };
    let rotation = values
        .get(3)
        .map(|value| number_required(source, Some(value), &format!("{label} rotation")))
        .transpose()?
        .unwrap_or(0.0);
    Ok((point, rotation))
}

fn parse_point(
    source: &str,
    expression: &SExpr,
    label: &str,
) -> Result<PointMm, FootprintImportError> {
    let values = expression.as_list().unwrap_or_default();
    Ok(PointMm {
        x: number_required(source, values.get(1), &format!("{label} x"))?,
        y: number_required(source, values.get(2), &format!("{label} y"))?,
    })
}

fn parse_optional_number(
    source: &str,
    items: &[SExpr],
    key: &str,
) -> Result<Option<f64>, FootprintImportError> {
    find_child(items, key)
        .map(|node| number_required(source, node.as_list().and_then(|values| values.get(1)), key))
        .transpose()
}

fn number_required(
    source: &str,
    value: Option<&SExpr>,
    label: &str,
) -> Result<f64, FootprintImportError> {
    let Some(value) = value else {
        return Err(FootprintImportError::data(format!("{label} is missing")));
    };
    let raw = value.as_text().ok_or_else(|| {
        FootprintImportError::at(source, value.start, format!("{label} must be a number"))
    })?;
    raw.parse::<f64>().map_err(|_| {
        FootprintImportError::at(
            source,
            value.start,
            format!("{label} '{raw}' is not a number"),
        )
    })
}

fn find_child<'a>(items: &'a [SExpr], key: &str) -> Option<&'a SExpr> {
    items.iter().find(|item| {
        item.as_list()
            .and_then(|values| values.first())
            .and_then(SExpr::as_text)
            == Some(key)
    })
}

fn feature(
    source: &str,
    location: impl Into<String>,
    name: impl Into<String>,
    expression: &SExpr,
    blocks_fabrication: bool,
) -> UnsupportedFootprintFeature {
    UnsupportedFootprintFeature {
        location: location.into(),
        feature: name.into(),
        raw_expression: source
            .get(expression.start..expression.end)
            .unwrap_or_default()
            .to_string(),
        blocks_fabrication,
    }
}

fn split_tags(value: &str) -> Vec<String> {
    value
        .split_ascii_whitespace()
        .map(str::to_owned)
        .filter(|tag| !tag.is_empty())
        .collect()
}

#[derive(Debug)]
enum SExprKind {
    List(Vec<SExpr>),
    Atom(String),
    String(String),
}

#[derive(Debug)]
struct SExpr {
    kind: SExprKind,
    start: usize,
    end: usize,
}

impl SExpr {
    fn as_list(&self) -> Option<&[SExpr]> {
        match &self.kind {
            SExprKind::List(items) => Some(items),
            _ => None,
        }
    }

    fn as_text(&self) -> Option<&str> {
        match &self.kind {
            SExprKind::Atom(value) | SExprKind::String(value) => Some(value),
            SExprKind::List(_) => None,
        }
    }
}

struct SExprParser<'a> {
    source: &'a str,
    cursor: usize,
    node_count: usize,
}

impl<'a> SExprParser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            cursor: 0,
            node_count: 0,
        }
    }

    fn parse_document(mut self) -> Result<SExpr, FootprintImportError> {
        self.skip_space_and_comments();
        let root = self.parse_expression(0)?;
        self.skip_space_and_comments();
        if self.cursor < self.source.len() {
            return Err(FootprintImportError::at(
                self.source,
                self.cursor,
                "unexpected content after the footprint expression",
            ));
        }
        Ok(root)
    }

    fn parse_expression(&mut self, depth: usize) -> Result<SExpr, FootprintImportError> {
        self.skip_space_and_comments();
        let start = self.cursor;
        if depth > MAX_NESTING_DEPTH {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "S-expression nesting exceeds the supported limit",
            ));
        }
        if self.node_count >= MAX_SEXPR_NODES {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "footprint has too many S-expression nodes",
            ));
        }
        self.node_count += 1;
        match self.peek_char() {
            Some('(') => self.parse_list(depth),
            Some(')') => Err(FootprintImportError::at(
                self.source,
                start,
                "unexpected closing parenthesis",
            )),
            Some('"') => self.parse_string(),
            Some(_) => self.parse_atom(),
            None => Err(FootprintImportError::at(
                self.source,
                start,
                "unexpected end of file",
            )),
        }
    }

    fn parse_list(&mut self, depth: usize) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        self.advance_char();
        let mut items = Vec::new();
        loop {
            self.skip_space_and_comments();
            match self.peek_char() {
                Some(')') => {
                    self.advance_char();
                    return Ok(SExpr {
                        kind: SExprKind::List(items),
                        start,
                        end: self.cursor,
                    });
                }
                None => {
                    return Err(FootprintImportError::at(
                        self.source,
                        start,
                        "unclosed list; expected ')'",
                    ));
                }
                _ => items.push(self.parse_expression(depth + 1)?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        self.advance_char();
        let mut value = String::new();
        loop {
            match self.advance_char() {
                Some('"') => {
                    return Ok(SExpr {
                        kind: SExprKind::String(value),
                        start,
                        end: self.cursor,
                    });
                }
                Some('\\') => match self.advance_char() {
                    Some('"') => value.push('"'),
                    Some('\\') => value.push('\\'),
                    Some('n') => value.push('\n'),
                    Some('r') => value.push('\r'),
                    Some('t') => value.push('\t'),
                    Some(other) => {
                        value.push('\\');
                        value.push(other);
                    }
                    None => {
                        return Err(FootprintImportError::at(
                            self.source,
                            start,
                            "unterminated escape sequence in string",
                        ));
                    }
                },
                Some(character) => value.push(character),
                None => {
                    return Err(FootprintImportError::at(
                        self.source,
                        start,
                        "unterminated quoted string",
                    ));
                }
            }
        }
    }

    fn parse_atom(&mut self) -> Result<SExpr, FootprintImportError> {
        let start = self.cursor;
        while let Some(character) = self.peek_char() {
            if character.is_whitespace() || matches!(character, '(' | ')' | ';') {
                break;
            }
            self.advance_char();
        }
        if self.cursor == start {
            return Err(FootprintImportError::at(
                self.source,
                start,
                "expected an atom",
            ));
        }
        Ok(SExpr {
            kind: SExprKind::Atom(self.source[start..self.cursor].to_string()),
            start,
            end: self.cursor,
        })
    }

    fn skip_space_and_comments(&mut self) {
        loop {
            while self.peek_char().is_some_and(char::is_whitespace) {
                self.advance_char();
            }
            if self.peek_char() == Some(';') {
                while let Some(character) = self.advance_char() {
                    if character == '\n' {
                        break;
                    }
                }
            } else {
                break;
            }
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source.get(self.cursor..)?.chars().next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let character = self.peek_char()?;
        self.cursor += character.len_utf8();
        Some(character)
    }
}

/// Project-local collection under `ElectricalAssets/Footprints`.
pub struct FootprintLibrary {
    directory: PathBuf,
}

impl FootprintLibrary {
    /// Import downloaded source while retaining its URL and library license.
    /// Parsing and collision checks are identical to a local import.
    pub fn import_kicad_source_with_provenance(
        &self,
        source: &str,
        provenance: FootprintProvenance,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let mut asset = parse_kicad_footprint(source)?;
        asset.provenance.source_path = provenance.source_path;
        asset.provenance.library = provenance.library;
        asset.provenance.license = asset.provenance.license.or(provenance.license);
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|e| FootprintLibraryError::Serialization(e.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }
    /// Opens (and creates when needed) the project-local collection.
    pub fn open(project_root: impl AsRef<Path>) -> Result<Self, FootprintLibraryError> {
        let directory = project_root
            .as_ref()
            .join(crate::library::ELECTRICAL_ASSETS_DIR)
            .join("Footprints");
        fs::create_dir_all(&directory)?;
        Ok(Self { directory })
    }

    /// Returns the on-disk collection directory.
    pub fn directory(&self) -> &Path {
        &self.directory
    }

    /// Imports one footprint string and atomically stores it as a `.ron` asset.
    /// Existing names are rejected so an import never silently replaces data.
    pub fn import_kicad_footprint(
        &self,
        source: &str,
        name_override: Option<&str>,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let mut asset = parse_kicad_footprint(source)?;
        if let Some(name) = name_override {
            if name.trim().is_empty() {
                return Err(FootprintLibraryError::InvalidName(name.to_string()));
            }
            asset.name = name.trim().to_string();
        }
        asset.id = safe_asset_stem(&asset.name)
            .ok_or_else(|| FootprintLibraryError::InvalidName(asset.name.clone()))?;
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|error| FootprintLibraryError::Serialization(error.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }

    /// Imports a `.kicad_mod` file and records its source path and `.pretty`
    /// folder name as provenance.
    pub fn import_kicad_file(
        &self,
        source_path: impl AsRef<Path>,
        name_override: Option<&str>,
    ) -> Result<FootprintAsset, FootprintLibraryError> {
        let source_path = source_path.as_ref();
        let source = read_limited_text(source_path, MAX_FOOTPRINT_SOURCE_BYTES)?;
        let mut asset = parse_kicad_footprint(&source)?;
        if let Some(name) = name_override {
            if name.trim().is_empty() {
                return Err(FootprintLibraryError::InvalidName(name.to_string()));
            }
            asset.name = name.trim().to_string();
        }
        asset.id = safe_asset_stem(&asset.name)
            .ok_or_else(|| FootprintLibraryError::InvalidName(asset.name.clone()))?;
        asset.provenance.source_path = Some(source_path.to_string_lossy().into_owned());
        asset.provenance.library = source_path
            .parent()
            .and_then(Path::file_name)
            .and_then(|name| name.to_str())
            .map(str::to_owned);
        let path = self.asset_path(&asset.id);
        if self.name_exists(&asset.name)? || path.exists() {
            return Err(FootprintLibraryError::DuplicateName(asset.name));
        }
        let serialized = ron::ser::to_string_pretty(&asset, ron::ser::PrettyConfig::default())
            .map_err(|error| FootprintLibraryError::Serialization(error.to_string()))?;
        write_new_atomic(&path, serialized.as_bytes())?;
        Ok(asset)
    }

    /// Loads one asset by its display name or stable local id.
    pub fn load(&self, name_or_id: &str) -> Result<Option<FootprintAsset>, FootprintLibraryError> {
        for asset in self.load_all()? {
            if asset.name.eq_ignore_ascii_case(name_or_id)
                || asset.id.eq_ignore_ascii_case(name_or_id)
            {
                return Ok(Some(asset));
            }
        }
        Ok(None)
    }

    /// Loads every saved footprint in stable name order.
    pub fn load_all(&self) -> Result<Vec<FootprintAsset>, FootprintLibraryError> {
        let mut assets = Vec::new();
        let mut total_bytes = 0usize;
        let mut seen_names = BTreeSet::new();
        let mut seen_ids = BTreeSet::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if !entry.file_type()?.is_file()
                || path.extension().and_then(|ext| ext.to_str()) != Some("ron")
            {
                continue;
            }
            if assets.len() >= MAX_LIBRARY_ASSETS {
                return Err(FootprintLibraryError::TooManyAssets {
                    directory: self.directory.clone(),
                    limit: MAX_LIBRARY_ASSETS,
                });
            }
            let contents = read_limited_text(&path, MAX_STORED_ASSET_BYTES)?;
            total_bytes = total_bytes.saturating_add(contents.len());
            if total_bytes > MAX_LIBRARY_TOTAL_BYTES {
                return Err(FootprintLibraryError::ResourceLimit {
                    path: self.directory.clone(),
                    limit_bytes: MAX_LIBRARY_TOTAL_BYTES,
                });
            }
            let asset: FootprintAsset =
                ron::from_str(&contents).map_err(|error| FootprintLibraryError::CorruptAsset {
                    path: path.clone(),
                    message: error.to_string(),
                })?;
            let issues = asset.validate();
            if !issues.is_empty() {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: issues
                        .iter()
                        .map(|issue| format!("{}: {}", issue.location, issue.message))
                        .collect::<Vec<_>>()
                        .join("; "),
                });
            }
            let file_id = path.file_stem().and_then(|stem| stem.to_str());
            if file_id != Some(asset.id.as_str())
                || safe_asset_stem(&asset.name).as_deref() != Some(asset.id.as_str())
            {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: "saved asset name, id, and filename do not match".into(),
                });
            }
            if !seen_names.insert(asset.name.to_ascii_lowercase())
                || !seen_ids.insert(asset.id.to_ascii_lowercase())
            {
                return Err(FootprintLibraryError::CorruptAsset {
                    path,
                    message: "duplicate saved footprint name or id".into(),
                });
            }
            assets.push(asset);
        }
        assets.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        Ok(assets)
    }

    /// Case-insensitive search across name, description, tags, and pad numbers.
    pub fn search(&self, query: &str) -> Result<Vec<FootprintAsset>, FootprintLibraryError> {
        let query = query.trim().to_ascii_lowercase();
        let mut matches = self
            .load_all()?
            .into_iter()
            .filter(|asset| {
                query.is_empty()
                    || asset.name.to_ascii_lowercase().contains(&query)
                    || asset.description.as_deref().is_some_and(|description| {
                        description.to_ascii_lowercase().contains(&query)
                    })
                    || asset
                        .tags
                        .iter()
                        .any(|tag| tag.to_ascii_lowercase().contains(&query))
                    || asset
                        .pads
                        .iter()
                        .any(|pad| pad.number.to_ascii_lowercase().contains(&query))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|left, right| {
            left.name
                .to_ascii_lowercase()
                .cmp(&right.name.to_ascii_lowercase())
        });
        Ok(matches)
    }

    fn asset_path(&self, id: &str) -> PathBuf {
        self.directory.join(format!("{id}.ron"))
    }

    fn name_exists(&self, name: &str) -> Result<bool, FootprintLibraryError> {
        Ok(self
            .load_all()?
            .iter()
            .any(|asset| asset.name.eq_ignore_ascii_case(name)))
    }
}

fn safe_asset_stem(name: &str) -> Option<String> {
    let mut output = String::with_capacity(name.len());
    let mut previous_separator = false;
    for character in name.trim().chars() {
        if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
            output.push(character.to_ascii_lowercase());
            previous_separator = false;
        } else if character.is_whitespace()
            || character == '.'
            || character == '/'
            || character == '\\'
        {
            if !previous_separator && !output.is_empty() {
                output.push('_');
                previous_separator = true;
            }
        } else {
            if !previous_separator && !output.is_empty() {
                output.push('_');
                previous_separator = true;
            }
        }
    }
    while output.ends_with('_') {
        output.pop();
    }
    if output.is_empty() || output == "." || output == ".." || output.len() > MAX_ASSET_ID_BYTES {
        None
    } else {
        Some(output)
    }
}

fn read_limited_text(path: &Path, limit_bytes: usize) -> Result<String, FootprintLibraryError> {
    let file = fs::File::open(path)?;
    let mut bytes = Vec::new();
    file.take(limit_bytes as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > limit_bytes {
        return Err(FootprintLibraryError::ResourceLimit {
            path: path.to_path_buf(),
            limit_bytes,
        });
    }
    String::from_utf8(bytes).map_err(|error| {
        FootprintLibraryError::Io(io::Error::new(io::ErrorKind::InvalidData, error))
    })
}

fn write_new_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut temporary_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "footprint".to_string());
    temporary_name.push_str(&format!(".{}.tmp", Uuid::new_v4()));
    let temporary = path.with_file_name(temporary_name);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let write_result = (|| -> io::Result<()> {
        file.write_all(contents)?;
        file.flush()?;
        file.sync_all()?;
        Ok(())
    })();
    drop(file);
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    // A same-directory hard link publishes the complete file atomically and
    // fails if another importer has already claimed the destination.
    let publish_result = fs::hard_link(&temporary, path);
    let _ = fs::remove_file(&temporary);
    match publish_result {
        Ok(()) => Ok(()),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"; imported fixture with an escaped quote: \"part\"
(footprint "Test:Header 2.54mm"
  (version 20240108)
  (generator "pcbnew")
  (descr "Two pad \"header\"")
  (tags "header pin through-hole")
  (property "License" "CC0-1.0")
  (fp_line (start -1 -2) (end 1 -2) (stroke (width 0.12) (type solid)) (layer "F.SilkS"))
  (fp_rect (start -1 -1) (end 1 1) (stroke (width 0.1) (type default)) (fill none) (layer "F.Fab"))
  (fp_circle (center 0 0) (end 0.5 0) (stroke (width 0.1) (type solid)) (fill none) (layer "F.CrtYd"))
  (fp_text reference "REF**" (at 0 -3 90) (layer "F.SilkS") (effects (font (size 1 1) (thickness 0.15))))
  (pad "1" thru_hole circle (at -1.27 0 90) (size 1.8 1.8) (drill 0.8) (layers "*.Cu" "*.Mask"))
  (pad "2" thru_hole oval (at 1.27 0) (size 1.8 2.1) (drill oval 0.8 1.2 (offset 0.1 0)) (layers "*.Cu" "*.Mask"))
)"#;

    fn scratch_dir() -> PathBuf {
        std::env::temp_dir().join(format!("raf_footprints_{}", Uuid::new_v4()))
    }

    #[test]
    fn parses_numbered_pads_rotations_drills_layers_and_manufacturing_graphics() {
        let footprint = parse_kicad_footprint(SAMPLE).expect("valid fixture");
        assert_eq!(footprint.name, "Test:Header 2.54mm");
        assert_eq!(footprint.provenance.format_version, Some(20240108));
        assert_eq!(footprint.provenance.license.as_deref(), Some("CC0-1.0"));
        assert_eq!(footprint.pads.len(), 2);
        assert_eq!(footprint.pads[0].number, "1");
        assert_eq!(footprint.pads[0].position_mm, PointMm { x: -1.27, y: 0.0 });
        assert_eq!(footprint.pads[0].rotation_degrees, 90.0);
        assert!(matches!(footprint.pads[0].shape, FootprintPadShape::Circle));
        assert!(matches!(
            footprint.pads[0].technology,
            FootprintPadTechnology::PlatedThroughHole
        ));
        assert!(matches!(
            footprint.pads[0].drill.as_ref().map(|drill| &drill.shape),
            Some(FootprintDrillShape::Circular { diameter_mm: 0.8 })
        ));
        assert!(footprint.pads[0]
            .layers
            .contains(&FootprintLayer::AllCopper));
        assert!(footprint.pads[0].layers.contains(&FootprintLayer::AllMask));
        assert!(matches!(
            footprint.pads[1].drill.as_ref().map(|drill| &drill.shape),
            Some(FootprintDrillShape::Oval {
                width_mm: 0.8,
                height_mm: 1.2
            })
        ));
        assert_eq!(footprint.graphics.len(), 4);
        assert!(footprint.is_fabrication_ready());
    }

    #[test]
    fn unsupported_pad_shapes_are_reported_and_block_fabrication() {
        let source = r#"(footprint "Custom"
          (pad "1" smd custom (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste")
            (options (clearance outline) (anchor rect))
            (primitives (gr_poly (pts (xy 0 0) (xy 1 0) (xy 0 1)) (width 0) (fill yes))))
        )"#;
        let footprint = parse_kicad_footprint(source).expect("valid but unsupported feature");
        assert!(matches!(
            &footprint.pads[0].shape,
            FootprintPadShape::Unsupported { source_shape } if source_shape == "custom"
        ));
        assert!(footprint
            .unsupported_features
            .iter()
            .any(|feature| feature.blocks_fabrication && feature.feature.contains("custom")));
        assert!(!footprint.is_fabrication_ready());
    }

    #[test]
    fn comments_escaped_strings_and_malformed_input_have_clear_behavior() {
        let source = r#"; top comment
(footprint "Escaped \"name\"" (property "license" "MIT\\compatible")
  (pad "1" smd rect (at 0 0) (size 1 1) (layers "F.Cu" "F.Mask" "F.Paste"))
)"#;
        let footprint = parse_kicad_footprint(source).expect("escaped input parses");
        assert_eq!(footprint.name, "Escaped \"name\"");
        assert!(footprint.provenance.license.unwrap().contains("\\"));
        let error = parse_kicad_footprint(
            "(footprint \"Broken\"\n  (pad \"1\" smd rect (at 0 0) (size 1 1)",
        )
        .expect_err("unclosed list must fail");
        assert!(error.message.contains("unclosed list"));
        assert_eq!(error.line, Some(2));
    }

    #[test]
    fn parser_enforces_input_limits_and_asset_ids_cannot_escape_the_library() {
        let oversized = " ".repeat(MAX_FOOTPRINT_SOURCE_BYTES + 1);
        let error = parse_kicad_footprint(&oversized).unwrap_err();
        assert!(error.message.contains("size limit"));

        let traversal = safe_asset_stem("../../outside").expect("sanitized local id");
        assert!(!traversal.contains('/') && !traversal.contains('\\'));
        assert!(!traversal.contains(".."));
        assert_eq!(traversal, "outside");
    }

    #[test]
    fn invalid_dimensions_and_drills_are_rejected_with_field_names() {
        let invalid_size = r#"(footprint "BadSize"
          (pad "A1" smd rect (at 0 0) (size 0 1) (layers "F.Cu" "F.Mask"))
        )"#;
        assert!(parse_kicad_footprint(invalid_size)
            .unwrap_err()
            .message
            .contains("size_mm"));
        let missing_drill = r#"(footprint "BadDrill"
          (pad "1" thru_hole circle (at 0 0) (size 1 1) (layers "*.Cu" "*.Mask"))
        )"#;
        assert!(parse_kicad_footprint(missing_drill)
            .unwrap_err()
            .message
            .contains("requires a drill"));
        let non_finite = r#"(footprint "BadNumber"
          (pad "1" smd rect (at nan 0) (size 1 1) (layers "F.Cu"))
        )"#;
        assert!(parse_kicad_footprint(non_finite)
            .unwrap_err()
            .message
            .contains("finite"));
    }

    #[test]
    fn local_library_import_search_and_ron_round_trip_preserve_source() {
        let root = scratch_dir();
        let library = FootprintLibrary::open(&root).expect("open library");
        let imported = library
            .import_kicad_footprint(SAMPLE, None)
            .expect("import asset");
        assert_eq!(
            imported.source_text, SAMPLE,
            "source text is retained byte-for-byte"
        );
        assert_eq!(library.search("header").unwrap().len(), 1);
        let loaded = library
            .load("test_header_2_54mm")
            .unwrap()
            .expect("load by id");
        assert_eq!(loaded, imported);
        assert!(library.import_kicad_footprint(SAMPLE, None).is_err());
        fs::remove_dir_all(root).expect("cleanup scratch directory");
    }
}
