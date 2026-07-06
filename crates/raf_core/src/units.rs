//! AuraRafi unit system.
//!
//! Canonical scale: 1 world unit = 1 meter.
//! Schematic/PCB canvas: 1 schematic unit = 1 millimeter.
//!
//! All internal calculations are done in SI units (meters, seconds, kilograms).
//! The `DisplayUnit` enum only controls how values are presented to the user
//! in the UI; it never affects computation.
//!
//! Future scripting runtimes (Rust + C++ via FFI) must import these
//! constants so that scripts always operate in SI units without manual
//! conversion.

use serde::{Deserialize, Serialize};

/// Meters per world unit. The canonical scale factor.
///
/// 1.0 means 1 world unit = 1 meter. If a future project needs a different
/// base scale (e.g. Unreal-style centimeters), change this single constant
/// and every system that depends on it adapts automatically.
pub const METERS_PER_UNIT: f32 = 1.0;

/// Millimeters per schematic unit. The electronics canvas scale.
///
/// 1.0 means 1 schematic unit = 1 mm. A 100x100mm board occupies 100x100
/// schematic units. Standard JLCPCB/PCBWay Gerber output uses mm natively.
pub const MM_PER_SCHEMATIC_UNIT: f32 = 1.0;

/// Conversion factor from schematic units (mm) to world units (m).
///
/// Multiply a schematic coordinate by this to get its world-space equivalent.
/// Example: a component at schematic (50, 30) maps to world (0.05, 0.0, 0.03).
pub const SCHEMATIC_TO_WORLD: f32 = 0.001;

/// Default grid spacing for the 3D viewport, in meters.
pub const DEFAULT_GRID_SPACING_M: f32 = 1.0;

/// Default grid spacing for the schematic canvas, in millimeters.
pub const DEFAULT_GRID_SPACING_MM: f32 = 1.0;

/// Snap options for the schematic grid, in millimeters.
pub const SCHEMATIC_SNAP_OPTIONS_MM: [f32; 4] = [0.5, 1.0, 2.54, 5.0];

/// Standard trace width for PCB, in millimeters (JLCPCB minimum).
pub const DEFAULT_TRACE_WIDTH_MM: f32 = 0.25;

/// Standard pad spacing for through-hole DIP, in millimeters.
pub const DEFAULT_PAD_SPACING_MM: f32 = 2.54;

/// How the UI displays world-space values. Does NOT affect computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DisplayUnit {
    /// Show values in meters / square meters / cubic meters.
    Metric,
    /// Show values in feet / square feet / cubic feet.
    Imperial,
    /// Show raw world units without a suffix (game-dev convention).
    Game,
}

impl Default for DisplayUnit {
    fn default() -> Self {
        Self::Metric
    }
}

impl DisplayUnit {
    /// Suffix shown next to a distance value in the UI.
    pub fn distance_suffix(self) -> &'static str {
        match self {
            Self::Metric => "m",
            Self::Imperial => "ft",
            Self::Game => "u",
        }
    }

    /// Suffix shown next to an area value in the UI.
    pub fn area_suffix(self) -> &'static str {
        match self {
            Self::Metric => "m2",
            Self::Imperial => "ft2",
            Self::Game => "u2",
        }
    }

    /// Suffix shown next to a volume value in the UI.
    pub fn volume_suffix(self) -> &'static str {
        match self {
            Self::Metric => "m3",
            Self::Imperial => "ft3",
            Self::Game => "u3",
        }
    }

    /// Convert a value stored in meters to the display unit.
    pub fn from_meters(self, meters: f32) -> f32 {
        match self {
            Self::Metric => meters,
            Self::Imperial => meters * 3.28084,
            Self::Game => meters,
        }
    }

    /// Convert a display value back to meters.
    pub fn to_meters(self, value: f32) -> f32 {
        match self {
            Self::Metric => value,
            Self::Imperial => value / 3.28084,
            Self::Game => value,
        }
    }

    /// Format a distance with the appropriate suffix.
    /// Example: Metric -> "1.80 m", Imperial -> "5.91 ft", Game -> "1.80 u"
    pub fn format_distance(self, meters: f32) -> String {
        let v = self.from_meters(meters);
        format!("{:.2} {}", v, self.distance_suffix())
    }

    /// Human-readable label for the settings dropdown.
    pub fn label(self) -> &'static str {
        match self {
            Self::Metric => "Metric (m)",
            Self::Imperial => "Imperial (ft)",
            Self::Game => "Game (units)",
        }
    }

    /// All variants for iteration in UI selectors.
    pub fn all() -> [DisplayUnit; 3] {
        [Self::Metric, Self::Imperial, Self::Game]
    }
}

/// Conversion helper: schematic units (mm) to world units (m).
pub fn schematic_to_world(schematic: f32) -> f32 {
    schematic * SCHEMATIC_TO_WORLD
}

/// Conversion helper: world units (m) to schematic units (mm).
pub fn world_to_schematic(world: f32) -> f32 {
    world / SCHEMATIC_TO_WORLD
}
