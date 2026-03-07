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
}

impl ElectronicComponent {
    /// Create a basic resistor.
    pub fn resistor(value: &str) -> Self {
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
        }
    }

    /// Create a basic capacitor.
    pub fn capacitor(value: &str) -> Self {
        let mut comp = Self::resistor(value);
        comp.designator = "C?".to_string();
        comp.category = "Passive".to_string();
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
        }
    }
}
