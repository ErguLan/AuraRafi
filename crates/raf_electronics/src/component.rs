//! Electronic component definition.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Pin direction for connection semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PinDirection {
    Input,
    Output,
    Bidirectional,
    Power,
    Ground,
}

/// Simulation model for a component.
///
/// Defines how the simulation engine treats each component type.
/// Only covers the 3 core components for now.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SimModel {
    /// Resistor with resistance in ohms.
    Resistor { ohms: f64 },
    /// Capacitor with capacitance in farads (DC steady-state = open circuit).
    Capacitor { farads: f64 },
    /// LED with a fixed forward voltage drop.
    Led { forward_voltage: f64 },
    /// Magnet with field strength in Tesla and polarity.
    /// Induces EMF in nearby conductors proportional to field strength.
    Magnet {
        /// Field strength in Tesla (e.g. 0.5 for a small neodymium).
        tesla: f64,
        /// Polarity: true = North facing up, false = South facing up.
        north_up: bool,
    },
    /// Simple wire / passthrough (zero resistance).
    Wire,
    /// DC Voltage Source (ideal battery).
    DcSource { voltage: f64 },
}

impl Default for SimModel {
    fn default() -> Self {
        SimModel::Wire
    }
}

/// A pin on an electronic component.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pin {
    pub id: Uuid,
    pub name: String,
    pub direction: PinDirection,
    /// Offset from component origin on the schematic grid.
    pub offset: Vec2,
    /// Net name this pin is connected to (or empty).
    pub net: String,
}

/// An electronic component (resistor, IC, transistor, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElectronicComponent {
    pub id: Uuid,
    /// Component designator (e.g. "R1", "U3").
    pub designator: String,
    /// Part name / value (e.g. "10k", "ATmega328P").
    pub value: String,
    /// Category for library browsing.
    pub category: String,
    /// Pins.
    pub pins: Vec<Pin>,
    /// Position on the schematic grid.
    pub position: Vec2,
    /// Rotation in degrees (0, 90, 180, 270).
    pub rotation: f32,
    /// Footprint reference for PCB (e.g. "0805", "DIP-28").
    pub footprint: String,
    /// Simulation model for this component.
    pub sim_model: SimModel,
}

/// Parse a human-readable value string into ohms.
///
/// Supports suffixes: k (kilo), M (mega), m (milli), u (micro).
/// Examples: "10k" -> 10_000.0, "4.7M" -> 4_700_000.0, "100" -> 100.0
pub fn parse_resistance(value: &str) -> f64 {
    parse_si_value(value).unwrap_or(1000.0)
}

/// Parse a human-readable capacitance string into farads.
///
/// Supports suffixes: u (micro), n (nano), p (pico), F (ignored).
/// Examples: "100nF" -> 100e-9, "10u" -> 10e-6, "22p" -> 22e-12
pub fn parse_capacitance(value: &str) -> f64 {
    parse_si_value(value).unwrap_or(100.0e-9)
}

/// Parse a value string with SI suffix into a numeric value.
fn parse_si_value(value: &str) -> Option<f64> {
    let s = value.trim().replace('F', "");
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Find where the numeric part ends.
    let mut num_end = s.len();
    for (i, ch) in s.char_indices().rev() {
        if ch.is_ascii_digit() || ch == '.' {
            num_end = i + ch.len_utf8();
            break;
        }
    }

    // If the entire string is a suffix with no digits, try treating
    // the suffix as the last char only.
    if num_end == 0 {
        return None;
    }

    let num_part = &s[..num_end];
    let suffix = s[num_end..].trim();

    let base: f64 = num_part.parse().ok()?;

    let multiplier = match suffix {
        "M" => 1_000_000.0,
        "k" | "K" => 1_000.0,
        "m" => 0.001,
        "u" | "uF" => 1e-6,
        "n" | "nF" => 1e-9,
        "p" | "pF" => 1e-12,
        "" => 1.0,
        _ => 1.0,
    };

    Some(base * multiplier)
}

impl ElectronicComponent {
    /// Create a basic resistor.
    pub fn resistor(value: &str) -> Self {
        let ohms = parse_resistance(value);
        Self {
            id: Uuid::new_v4(),
            designator: "R?".to_string(),
            value: value.to_string(),
            category: "Passive".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "1".to_string(),
                    direction: PinDirection::Bidirectional,
                    offset: Vec2::new(-1.0, 0.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "2".to_string(),
                    direction: PinDirection::Bidirectional,
                    offset: Vec2::new(1.0, 0.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "0805".to_string(),
            sim_model: SimModel::Resistor { ohms },
        }
    }

    /// Create a basic capacitor.
    pub fn capacitor(value: &str) -> Self {
        let farads = parse_capacitance(value);
        let mut comp = Self::resistor(value);
        comp.designator = "C?".to_string();
        comp.category = "Passive".to_string();
        comp.sim_model = SimModel::Capacitor { farads };
        comp
    }

    /// Create a basic LED.
    pub fn led() -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "D?".to_string(),
            value: "LED".to_string(),
            category: "Diode".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "A".to_string(),
                    direction: PinDirection::Input,
                    offset: Vec2::new(-1.0, 0.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "K".to_string(),
                    direction: PinDirection::Output,
                    offset: Vec2::new(1.0, 0.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "0805".to_string(),
            sim_model: SimModel::Led { forward_voltage: 2.0 },
        }
    }

    /// Create a magnet component.
    ///
    /// Magnets have North (N) and South (S) poles as pins.
    /// The field strength is specified as a human-readable label
    /// (e.g. "0.5T", "1T", "Weak", "Strong").
    /// They induce EMF in nearby conductors and attract/repel
    /// other magnets based on polarity.
    pub fn magnet(strength: &str) -> Self {
        let tesla = parse_magnet_strength(strength);
        Self {
            id: Uuid::new_v4(),
            designator: "MAG?".to_string(),
            value: strength.to_string(),
            category: "Magnet".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "N".to_string(),
                    direction: PinDirection::Output,
                    offset: Vec2::new(0.0, -1.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "S".to_string(),
                    direction: PinDirection::Input,
                    offset: Vec2::new(0.0, 1.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "MAG-10x5".to_string(),
            sim_model: SimModel::Magnet {
                tesla,
                north_up: true,
            },
        }
    }

    /// Create a DC Source (Battery).
    pub fn dc_source(voltage: f64) -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "V?".to_string(),
            value: format!("{}V", voltage),
            category: "Power".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "+".to_string(),
                    direction: PinDirection::Power,
                    offset: Vec2::new(0.0, -1.0),
                    net: String::new(),
                },
                Pin {
                    id: Uuid::new_v4(),
                    name: "-".to_string(),
                    direction: PinDirection::Power,
                    offset: Vec2::new(0.0, 1.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "BAT-18650".to_string(),
            sim_model: SimModel::DcSource { voltage },
        }
    }
    
    /// Create a Ground terminal.
    pub fn ground() -> Self {
        Self {
            id: Uuid::new_v4(),
            designator: "GND".to_string(),
            value: "GND".to_string(),
            category: "Power".to_string(),
            pins: vec![
                Pin {
                    id: Uuid::new_v4(),
                    name: "GND".to_string(),
                    direction: PinDirection::Ground,
                    offset: Vec2::new(0.0, 0.0),
                    net: String::new(),
                },
            ],
            position: Vec2::ZERO,
            rotation: 0.0,
            footprint: "TP-GND".to_string(),
            sim_model: SimModel::Wire, // electrically transparent
        }
    }
}

/// Parse a magnet strength string into Tesla.
/// Supports: "0.5T", "1T", "Weak" (0.1T), "Strong" (1.0T), "Neodymium" (0.5T).
fn parse_magnet_strength(value: &str) -> f64 {
    let s = value.trim().to_lowercase();
    match s.as_str() {
        "weak" => 0.1,
        "medium" => 0.3,
        "strong" => 1.0,
        "neodymium" => 0.5,
        _ => {
            // Try parsing as number with optional T suffix.
            let cleaned = s.replace('t', "");
            cleaned.parse::<f64>().unwrap_or(0.3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_resistance_values() {
        assert!((parse_resistance("10k") - 10_000.0).abs() < 0.1);
        assert!((parse_resistance("4.7k") - 4_700.0).abs() < 0.1);
        assert!((parse_resistance("1M") - 1_000_000.0).abs() < 0.1);
        assert!((parse_resistance("100") - 100.0).abs() < 0.1);
        assert!((parse_resistance("470") - 470.0).abs() < 0.1);
    }

    #[test]
    fn parse_capacitance_values() {
        assert!((parse_capacitance("100nF") - 100e-9).abs() < 1e-12);
        assert!((parse_capacitance("10u") - 10e-6).abs() < 1e-9);
        assert!((parse_capacitance("22p") - 22e-12).abs() < 1e-15);
    }

    #[test]
    fn resistor_has_sim_model() {
        let r = ElectronicComponent::resistor("10k");
        match r.sim_model {
            SimModel::Resistor { ohms } => assert!((ohms - 10_000.0).abs() < 0.1),
            _ => panic!("Expected Resistor sim model"),
        }
    }

    #[test]
    fn capacitor_has_sim_model() {
        let c = ElectronicComponent::capacitor("100nF");
        match c.sim_model {
            SimModel::Capacitor { farads } => assert!((farads - 100e-9).abs() < 1e-12),
            _ => panic!("Expected Capacitor sim model"),
        }
    }

    #[test]
    fn led_has_sim_model() {
        let l = ElectronicComponent::led();
        match l.sim_model {
            SimModel::Led { forward_voltage } => assert!((forward_voltage - 2.0).abs() < 0.01),
            _ => panic!("Expected Led sim model"),
        }
    }
}
