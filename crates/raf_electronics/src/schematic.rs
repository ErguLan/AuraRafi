//! Schematic data structure - components, wires, and nets.

use crate::component::ElectronicComponent;
use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A wire segment connecting two points on the schematic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wire {
    pub id: Uuid,
    pub start: Vec2,
    pub end: Vec2,
    pub net: String,
}

/// A complete schematic containing components and wires.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Schematic {
    pub name: String,
    pub components: Vec<ElectronicComponent>,
    pub wires: Vec<Wire>,
}

impl Schematic {
    /// Create an empty schematic.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            components: Vec::new(),
            wires: Vec::new(),
        }
    }

    /// Add a component to the schematic.
    pub fn add_component(&mut self, mut component: ElectronicComponent) -> Uuid {
        let id = component.id;
        // Auto-assign designator number.
        let prefix = component.designator.replace('?', "");
        let count = self
            .components
            .iter()
            .filter(|c| c.designator.starts_with(&prefix))
            .count();
        component.designator = format!("{}{}", prefix, count + 1);
        self.components.push(component);
        id
    }

    /// Add a wire between two points.
    pub fn add_wire(&mut self, start: Vec2, end: Vec2, net: &str) -> Uuid {
        let wire = Wire {
            id: Uuid::new_v4(),
            start,
            end,
            net: net.to_string(),
        };
        let id = wire.id;
        self.wires.push(wire);
        id
    }

    /// Run a basic electrical connectivity check.
    /// Returns a list of warnings/errors as strings.
    pub fn electrical_test(&self) -> Vec<String> {
        let mut issues = Vec::new();

        // Check for unconnected pins.
        for comp in &self.components {
            for pin in &comp.pins {
                if pin.net.is_empty() {
                    issues.push(format!(
                        "Unconnected pin: {} pin {} ({})",
                        comp.designator, pin.name, comp.value
                    ));
                }
            }
        }

        // Check for floating wires (wire with no components on either end).
        for wire in &self.wires {
            if wire.net.is_empty() {
                issues.push(format!(
                    "Unnamed net on wire from ({:.1},{:.1}) to ({:.1},{:.1})",
                    wire.start.x, wire.start.y, wire.end.x, wire.end.y
                ));
            }
        }

        if issues.is_empty() {
            issues.push("Electrical test passed - no issues found.".to_string());
        }

        issues
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;

    #[test]
    fn auto_designator() {
        let mut sch = Schematic::new("Test");
        sch.add_component(ElectronicComponent::resistor("10k"));
        sch.add_component(ElectronicComponent::resistor("4.7k"));
        assert_eq!(sch.components[0].designator, "R1");
        assert_eq!(sch.components[1].designator, "R2");
    }
}
