//! Authored vector symbols for schematic components.
//!
//! A symbol is deliberately a small, closed primitive set instead of a general
//! vector document: every custom part must stay renderable by the same CAD
//! pipeline that draws the built-in parts, with bounded work on low-end
//! hardware. Coordinates are expressed in schematic world units, so the same
//! definition can be drawn on the canvas, measured for picking and rasterized
//! for a library preview without a second representation.

use glam::Vec2;
use serde::{Deserialize, Serialize};

/// World units used by one schematic grid cell.
///
/// `Pin::offset` is expressed in grid cells, while every authored symbol
/// primitive is expressed in the world units this constant resolves. Keeping
/// the conversion in one place is what lets a generated symbol and its pins
/// share geometry.
pub const SYMBOL_GRID_UNIT: f32 = 20.0;

/// Multiplier from authored symbol space to schematic world space.
pub const DEFAULT_SYMBOL_SCALE: f32 = 1.0;

/// Maximum number of primitives one symbol may carry.
pub const MAX_SYMBOL_PRIMITIVES: usize = 256;
/// Maximum number of vertices in one authored polyline.
pub const MAX_SYMBOL_POLYLINE_POINTS: usize = 256;
/// Aggregate ink budget for one symbol, including generated circle segments.
pub const MAX_SYMBOL_TOTAL_POINTS: usize = 8_192;
/// Largest absolute coordinate a primitive may reference.
pub const MAX_SYMBOL_COORDINATE: f32 = 10_000.0;
/// Smallest valid circle radius.
pub const MIN_SYMBOL_RADIUS: f32 = 0.01;
/// Largest valid circle radius.
pub const MAX_SYMBOL_RADIUS: f32 = 1_000.0;
/// Longest literal text a symbol author may attach to a primitive.
pub const MAX_SYMBOL_TEXT_CHARS: usize = 32;
/// Segments used to convert a circle primitive into bounded path ink.
pub const SYMBOL_CIRCLE_SEGMENTS: usize = 24;
/// Maximum number of pins a generated preset will lay out.
pub const MAX_SYMBOL_PINS: usize = 256;

/// Half-width of the horizontal stroke used to mark authored text.
const SYMBOL_TEXT_TICK_HALF_WIDTH: f32 = 3.0;
/// Half-height of the horizontal stroke used to mark authored text.
const SYMBOL_TEXT_TICK_HALF_HEIGHT: f32 = 2.0;
/// Half-width of a passive body in world units.
const PASSIVE_BODY_HALF_WIDTH: f32 = 8.0;
/// Half-height of a passive body in world units.
const PASSIVE_BODY_HALF_HEIGHT: f32 = 6.0;
/// Half-width of a box-shaped component body in world units.
const BOX_BODY_HALF_WIDTH: f32 = 30.0;
/// Half-width of an IC body in world units. Kept inside the two-cell pin
/// offset so a generated IC never paints its outline over a pin mark.
const IC_BODY_HALF_WIDTH: f32 = 34.0;
/// Minimum half-height shared by generated rectangular bodies.
const BODY_MIN_HALF_HEIGHT: f32 = 20.0;
/// Vertical pitch between generated pins, in grid cells.
const GENERATED_PIN_PITCH: f32 = 2.0;
/// Horizontal offset of generated box pins, in grid cells.
const GENERATED_BOX_PIN_X: f32 = 2.0;
/// Horizontal offset of generated connector pins, in grid cells.
const GENERATED_CONNECTOR_PIN_X: f32 = -2.0;
/// Contact stripe inset inside a generated connector body.
const CONNECTOR_CONTACT_INSET: f32 = 10.0;
/// Radius of the orientation dot inside a generated IC body.
const IC_ORIENTATION_DOT_RADIUS: f32 = 3.0;
/// Half-width of the orientation notch on a generated IC body.
const IC_NOTCH_HALF_WIDTH: f32 = 6.0;

/// One authored vector primitive in schematic world space.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SymbolPrimitive {
    /// Independent straight stroke.
    Line { from: Vec2, to: Vec2 },
    /// Connected stroke with at least two vertices.
    Polyline {
        #[serde(default)]
        points: Vec<Vec2>,
    },
    /// Axis-aligned rectangle drawn as a closed outline.
    Rect { center: Vec2, size: Vec2 },
    /// Circle drawn as a closed polyline.
    Circle { center: Vec2, radius: f32 },
    /// Authored annotation. The CAD path renders it as a bounded tick because
    /// the schematic has no per-symbol font renderer.
    Text {
        at: Vec2,
        #[serde(default)]
        value: String,
    },
}

impl SymbolPrimitive {
    /// Number of path vertices this primitive contributes to the ink budget.
    pub fn point_count(&self) -> usize {
        match self {
            Self::Line { .. } => 2,
            Self::Polyline { points } => points.len(),
            Self::Rect { .. } => 5,
            Self::Circle { .. } => SYMBOL_CIRCLE_SEGMENTS + 1,
            Self::Text { .. } => 4,
        }
    }

    /// Validates this primitive in isolation.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Line { from, to } => {
                validate_point(*from)?;
                validate_point(*to)
            }
            Self::Polyline { points } => {
                if points.len() < 2 {
                    return Err("a symbol polyline needs at least two points".to_string());
                }
                if points.len() > MAX_SYMBOL_POLYLINE_POINTS {
                    return Err(format!(
                        "a symbol polyline is limited to {MAX_SYMBOL_POLYLINE_POINTS} points"
                    ));
                }
                for point in points {
                    validate_point(*point)?;
                }
                Ok(())
            }
            Self::Rect { center, size } => {
                validate_point(*center)?;
                if !size.is_finite() || size.x.abs() <= f32::EPSILON || size.y.abs() <= f32::EPSILON
                {
                    return Err("a symbol rectangle needs a finite, non-zero size".to_string());
                }
                let half = size.abs() * 0.5;
                validate_point(*center - half)?;
                validate_point(*center + half)
            }
            Self::Circle { center, radius } => {
                validate_point(*center)?;
                if !radius.is_finite() || !(MIN_SYMBOL_RADIUS..=MAX_SYMBOL_RADIUS).contains(radius)
                {
                    return Err(format!(
                        "a symbol circle radius must stay between {MIN_SYMBOL_RADIUS} and {MAX_SYMBOL_RADIUS}"
                    ));
                }
                Ok(())
            }
            Self::Text { at, value } => {
                validate_point(*at)?;
                if value.chars().count() > MAX_SYMBOL_TEXT_CHARS {
                    return Err(format!(
                        "symbol text is limited to {MAX_SYMBOL_TEXT_CHARS} characters"
                    ));
                }
                Ok(())
            }
        }
    }
}

/// Authored vector symbol attached to a custom component.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct SymbolDefinition {
    #[serde(default)]
    pub primitives: Vec<SymbolPrimitive>,
}

impl SymbolDefinition {
    /// Creates a symbol definition from its primitives.
    pub fn new(primitives: Vec<SymbolPrimitive>) -> Self {
        Self { primitives }
    }

    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// Validates the complete definition before it is persisted or rendered.
    ///
    /// The failure strings are developer diagnostics, not localized UI copy.
    pub fn validate(&self) -> Result<(), String> {
        if self.primitives.is_empty() {
            return Err("a symbol needs at least one primitive".to_string());
        }
        if self.primitives.len() > MAX_SYMBOL_PRIMITIVES {
            return Err(format!(
                "a symbol is limited to {MAX_SYMBOL_PRIMITIVES} primitives"
            ));
        }
        let mut total_points = 0usize;
        for primitive in &self.primitives {
            primitive.validate()?;
            total_points = total_points.saturating_add(primitive.point_count());
        }
        if total_points > MAX_SYMBOL_TOTAL_POINTS {
            return Err(format!(
                "symbol geometry exceeds the {MAX_SYMBOL_TOTAL_POINTS} point budget"
            ));
        }
        Ok(())
    }

    /// Converts the authored primitives into bounded path ink.
    ///
    /// Call [`Self::validate`] first when the definition comes from disk. An
    /// invalid definition still produces only the primitives it can represent,
    /// so a malformed radius or degenerate rectangle can never panic.
    pub fn to_paths(&self) -> Vec<Vec<Vec2>> {
        if self.primitives.len() > MAX_SYMBOL_PRIMITIVES {
            return Vec::new();
        }
        let mut paths = Vec::with_capacity(self.primitives.len());
        for primitive in &self.primitives {
            match primitive {
                SymbolPrimitive::Line { from, to } => {
                    if from.is_finite() && to.is_finite() {
                        paths.push(vec![*from, *to]);
                    }
                }
                SymbolPrimitive::Polyline { points } => {
                    if points.len() >= 2
                        && points.len() <= MAX_SYMBOL_POLYLINE_POINTS
                        && points.iter().all(|point| point.is_finite())
                    {
                        paths.push(points.clone());
                    }
                }
                SymbolPrimitive::Rect { center, size } => {
                    let half = size.abs() * 0.5;
                    if !center.is_finite()
                        || !size.is_finite()
                        || half.x <= f32::EPSILON
                        || half.y <= f32::EPSILON
                    {
                        continue;
                    }
                    let corners = [
                        *center + Vec2::new(-half.x, -half.y),
                        *center + Vec2::new(half.x, -half.y),
                        *center + Vec2::new(half.x, half.y),
                        *center + Vec2::new(-half.x, half.y),
                        *center + Vec2::new(-half.x, -half.y),
                    ];
                    if corners.iter().all(|point| point.is_finite()) {
                        paths.push(corners.to_vec());
                    }
                }
                SymbolPrimitive::Circle { center, radius } => {
                    if !center.is_finite() || !radius.is_finite() || *radius <= f32::EPSILON {
                        continue;
                    }
                    let mut points = Vec::with_capacity(SYMBOL_CIRCLE_SEGMENTS + 1);
                    for step in 0..=SYMBOL_CIRCLE_SEGMENTS {
                        let angle =
                            step as f32 / SYMBOL_CIRCLE_SEGMENTS as f32 * std::f32::consts::TAU;
                        points.push(*center + Vec2::new(angle.cos(), angle.sin()) * *radius);
                    }
                    paths.push(points);
                }
                SymbolPrimitive::Text { at, .. } => {
                    if at.is_finite() {
                        // Literal text is drawn by the component label path.
                        // The symbol itself contributes a small, deterministic
                        // tick so authored annotations remain visible in the
                        // preview and on the canvas without a font renderer.
                        paths.push(vec![
                            *at + Vec2::new(
                                -SYMBOL_TEXT_TICK_HALF_WIDTH,
                                SYMBOL_TEXT_TICK_HALF_HEIGHT,
                            ),
                            *at + Vec2::new(
                                SYMBOL_TEXT_TICK_HALF_WIDTH,
                                SYMBOL_TEXT_TICK_HALF_HEIGHT,
                            ),
                        ]);
                        paths.push(vec![
                            *at + Vec2::new(0.0, SYMBOL_TEXT_TICK_HALF_HEIGHT),
                            *at - Vec2::new(0.0, SYMBOL_TEXT_TICK_HALF_HEIGHT),
                        ]);
                    }
                }
            }
        }
        paths
    }
}

/// Preset body styles available to a custom component author.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SymbolPreset {
    /// Two-terminal passive body.
    #[default]
    Passive,
    /// Plain rectangular body with pins on both sides.
    Box,
    /// Rectangular body with an orientation dot and notch.
    Ic,
    /// Single-column connector body.
    Connector,
    /// Single-pin ground reference.
    Ground,
}

impl SymbolPreset {
    /// Generates a bounded symbol definition for this preset.
    ///
    /// Generated bodies never overlap their pins: the pin marks are drawn by
    /// the CAD layer at `pin_offsets(pin_count) * SYMBOL_GRID_UNIT`, and the
    /// body is inset at least one grid unit from the outermost pin row.
    pub fn symbol_for(self, pin_count: usize) -> SymbolDefinition {
        let offsets = self.pin_offsets(pin_count);
        let pins = offsets
            .iter()
            .map(|offset| *offset * SYMBOL_GRID_UNIT)
            .collect::<Vec<_>>();
        let primitives = match self {
            Self::Passive if pins.len() == 2 => passive_symbol(&pins),
            Self::Passive if pins.len() == 1 => single_terminal_symbol(pins[0]),
            Self::Passive => box_symbol(&pins, false),
            Self::Box => box_symbol(&pins, false),
            Self::Ic => box_symbol(&pins, true),
            Self::Connector => connector_symbol(&pins),
            Self::Ground => ground_symbol(),
        };
        SymbolDefinition::new(primitives)
    }

    /// Pin offsets in schematic grid cells for this preset.
    ///
    /// The result is always bounded to [`MAX_SYMBOL_PINS`] entries. A ground
    /// preset always resolves to its single reference pin.
    pub fn pin_offsets(self, pin_count: usize) -> Vec<Vec2> {
        let count = pin_count.clamp(1, MAX_SYMBOL_PINS);
        match self {
            Self::Passive => match count {
                1 => vec![Vec2::ZERO],
                2 => vec![Vec2::new(-1.0, 0.0), Vec2::new(1.0, 0.0)],
                _ => box_pin_offsets(count),
            },
            Self::Box | Self::Ic => box_pin_offsets(count),
            Self::Connector => (0..count)
                .map(|index| Vec2::new(GENERATED_CONNECTOR_PIN_X, column_offset(index, count)))
                .collect(),
            Self::Ground => vec![Vec2::ZERO],
        }
    }
}

fn validate_point(point: Vec2) -> Result<(), String> {
    if !point.is_finite()
        || point.x.abs() > MAX_SYMBOL_COORDINATE
        || point.y.abs() > MAX_SYMBOL_COORDINATE
    {
        return Err(format!(
            "symbol coordinates must be finite and within +/-{MAX_SYMBOL_COORDINATE}"
        ));
    }
    Ok(())
}

fn column_offset(index: usize, count: usize) -> f32 {
    if count <= 1 {
        0.0
    } else {
        index as f32 * GENERATED_PIN_PITCH - (count as f32 - 1.0)
    }
}

fn box_pin_offsets(count: usize) -> Vec<Vec2> {
    let left = (count + 1) / 2;
    let right = count / 2;
    let mut offsets = Vec::with_capacity(count);
    for index in 0..left {
        offsets.push(Vec2::new(-GENERATED_BOX_PIN_X, column_offset(index, left)));
    }
    for index in 0..right {
        offsets.push(Vec2::new(GENERATED_BOX_PIN_X, column_offset(index, right)));
    }
    offsets
}

fn passive_symbol(pins: &[Vec2]) -> Vec<SymbolPrimitive> {
    let left = pins[0];
    let right = pins[1];
    vec![
        SymbolPrimitive::Line {
            from: left,
            to: Vec2::new(-PASSIVE_BODY_HALF_WIDTH, 0.0),
        },
        SymbolPrimitive::Rect {
            center: Vec2::ZERO,
            size: Vec2::new(
                PASSIVE_BODY_HALF_WIDTH * 2.0,
                PASSIVE_BODY_HALF_HEIGHT * 2.0,
            ),
        },
        SymbolPrimitive::Line {
            from: Vec2::new(PASSIVE_BODY_HALF_WIDTH, 0.0),
            to: right,
        },
    ]
}

fn single_terminal_symbol(pin: Vec2) -> Vec<SymbolPrimitive> {
    vec![
        SymbolPrimitive::Line {
            from: pin,
            to: pin + Vec2::Y * PASSIVE_BODY_HALF_HEIGHT,
        },
        SymbolPrimitive::Rect {
            center: pin + Vec2::Y * PASSIVE_BODY_HALF_HEIGHT * 2.0,
            size: Vec2::new(
                PASSIVE_BODY_HALF_WIDTH * 2.0,
                PASSIVE_BODY_HALF_HEIGHT * 2.0,
            ),
        },
    ]
}

fn box_symbol(pins: &[Vec2], integrated_circuit: bool) -> Vec<SymbolPrimitive> {
    let half_width = if integrated_circuit {
        IC_BODY_HALF_WIDTH
    } else {
        BOX_BODY_HALF_WIDTH
    };
    let half_height = body_half_height(pins);
    let mut primitives = vec![SymbolPrimitive::Rect {
        center: Vec2::ZERO,
        size: Vec2::new(half_width * 2.0, half_height * 2.0),
    }];
    if integrated_circuit {
        primitives.push(SymbolPrimitive::Circle {
            center: Vec2::new(
                -half_width + CONNECTOR_CONTACT_INSET,
                -half_height + CONNECTOR_CONTACT_INSET,
            ),
            radius: IC_ORIENTATION_DOT_RADIUS,
        });
        primitives.push(SymbolPrimitive::Line {
            from: Vec2::new(-IC_NOTCH_HALF_WIDTH, -half_height),
            to: Vec2::new(IC_NOTCH_HALF_WIDTH, -half_height),
        });
    }
    primitives
}

fn connector_symbol(pins: &[Vec2]) -> Vec<SymbolPrimitive> {
    let half_width = BOX_BODY_HALF_WIDTH;
    let half_height = body_half_height(pins);
    vec![
        SymbolPrimitive::Rect {
            center: Vec2::ZERO,
            size: Vec2::new(half_width * 2.0, half_height * 2.0),
        },
        SymbolPrimitive::Line {
            from: Vec2::new(
                -half_width + CONNECTOR_CONTACT_INSET,
                -half_height + CONNECTOR_CONTACT_INSET,
            ),
            to: Vec2::new(
                -half_width + CONNECTOR_CONTACT_INSET,
                half_height - CONNECTOR_CONTACT_INSET,
            ),
        },
    ]
}

fn body_half_height(pins: &[Vec2]) -> f32 {
    let outermost = pins.iter().map(|pin| pin.y.abs()).fold(0.0_f32, f32::max);
    (outermost + SYMBOL_GRID_UNIT).max(BODY_MIN_HALF_HEIGHT)
}

fn ground_symbol() -> Vec<SymbolPrimitive> {
    vec![
        SymbolPrimitive::Line {
            from: Vec2::ZERO,
            to: Vec2::Y * 10.0,
        },
        SymbolPrimitive::Line {
            from: Vec2::new(-9.0, 10.0),
            to: Vec2::new(9.0, 10.0),
        },
        SymbolPrimitive::Line {
            from: Vec2::new(-6.0, 14.0),
            to: Vec2::new(6.0, 14.0),
        },
        SymbolPrimitive::Line {
            from: Vec2::new(-3.0, 18.0),
            to: Vec2::new(3.0, 18.0),
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_generates_a_valid_bounded_symbol() {
        for preset in [
            SymbolPreset::Passive,
            SymbolPreset::Box,
            SymbolPreset::Ic,
            SymbolPreset::Connector,
            SymbolPreset::Ground,
        ] {
            for pin_count in [1usize, 2, 3, 8, 64, MAX_SYMBOL_PINS] {
                let symbol = preset.symbol_for(pin_count);
                symbol
                    .validate()
                    .unwrap_or_else(|error| panic!("{preset:?} x {pin_count}: {error}"));
                assert!(!symbol.to_paths().is_empty());
                assert!(preset.pin_offsets(pin_count).len() <= MAX_SYMBOL_PINS);
            }
        }
    }

    #[test]
    fn generated_bodies_stay_clear_of_their_pins() {
        let symbol = SymbolPreset::Ic.symbol_for(16);
        let offsets = SymbolPreset::Ic.pin_offsets(16);
        let half_height = body_half_height(
            &offsets
                .iter()
                .map(|offset| *offset * SYMBOL_GRID_UNIT)
                .collect::<Vec<_>>(),
        );
        let body = symbol
            .primitives
            .iter()
            .find_map(|primitive| match primitive {
                SymbolPrimitive::Rect { size, .. } => Some(*size),
                _ => None,
            })
            .expect("IC body");
        assert!(body.y * 0.5 > half_height - SYMBOL_GRID_UNIT);
        for offset in offsets {
            let pin = offset * SYMBOL_GRID_UNIT;
            assert!(pin.y.abs() < body.y * 0.5);
            assert!(pin.x.abs() > body.x * 0.5);
        }
    }

    #[test]
    fn validation_rejects_unbounded_authoring_input() {
        let too_many = SymbolDefinition::new(
            (0..=MAX_SYMBOL_PRIMITIVES)
                .map(|_| SymbolPrimitive::Line {
                    from: Vec2::ZERO,
                    to: Vec2::X,
                })
                .collect(),
        );
        assert!(too_many.validate().is_err());

        let long_text = SymbolDefinition::new(vec![SymbolPrimitive::Text {
            at: Vec2::ZERO,
            value: "x".repeat(MAX_SYMBOL_TEXT_CHARS + 1),
        }]);
        assert!(long_text.validate().is_err());

        let bad_radius = SymbolDefinition::new(vec![SymbolPrimitive::Circle {
            center: Vec2::ZERO,
            radius: MAX_SYMBOL_RADIUS + 1.0,
        }]);
        assert!(bad_radius.validate().is_err());
    }

    #[test]
    fn rect_and_circle_convert_to_closed_path_ink() {
        let symbol = SymbolDefinition::new(vec![
            SymbolPrimitive::Rect {
                center: Vec2::new(10.0, 10.0),
                size: Vec2::new(20.0, 10.0),
            },
            SymbolPrimitive::Circle {
                center: Vec2::ZERO,
                radius: 5.0,
            },
        ]);
        let paths = symbol.to_paths();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].first(), paths[0].last());
        assert_eq!(paths[1].len(), SYMBOL_CIRCLE_SEGMENTS + 1);
        let first = paths[1].first().copied().unwrap_or(Vec2::ZERO);
        let last = paths[1].last().copied().unwrap_or(Vec2::ZERO);
        assert!(first.distance(last) < 0.001);
    }
}
