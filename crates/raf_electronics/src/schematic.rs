//! Schematic data structure - components, wires, and nets.

use crate::component::ElectronicComponent;
use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const WIRE_POINT_EPSILON: f32 = 0.001;
const SCHEMATIC_GRID_STEP: f32 = 20.0;

/// Optional endpoint attachment used by the editor to keep wires connected
/// when components move. `start` and `end` remain the persisted fallback.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WireAnchor {
    Pin { component_id: Uuid, pin_id: Uuid },
    Point(Vec2),
}

/// A wire segment connecting two points on the schematic.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wire {
    pub id: Uuid,
    pub start: Vec2,
    pub end: Vec2,
    pub net: String,
    #[serde(default)]
    pub start_anchor: Option<WireAnchor>,
    #[serde(default)]
    pub end_anchor: Option<WireAnchor>,
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
            start_anchor: None,
            end_anchor: None,
        };
        let id = wire.id;
        self.wires.push(wire);
        id
    }

    /// Add a multi-segment wire path, skipping zero-length segments.
    pub fn add_wire_path(&mut self, points: &[Vec2], net: &str) -> usize {
        let mut inserted = 0;

        for pair in points.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start.distance(end) <= WIRE_POINT_EPSILON {
                continue;
            }

            self.add_wire(start, end, net);
            inserted += 1;
        }

        inserted
    }

    /// Add a wire with optional editor anchors for pin-aware routing.
    pub fn add_wire_anchored(
        &mut self,
        start: Vec2,
        end: Vec2,
        net: &str,
        start_anchor: Option<WireAnchor>,
        end_anchor: Option<WireAnchor>,
    ) -> Uuid {
        let wire = Wire {
            id: Uuid::new_v4(),
            start,
            end,
            net: net.to_string(),
            start_anchor,
            end_anchor,
        };
        let id = wire.id;
        self.wires.push(wire);
        id
    }

    /// Add a routed path and attach the first/last segment to endpoints when provided.
    pub fn add_wire_path_anchored(
        &mut self,
        points: &[Vec2],
        net: &str,
        start_anchor: Option<WireAnchor>,
        end_anchor: Option<WireAnchor>,
    ) -> usize {
        let mut inserted = 0;
        let segment_count = points
            .windows(2)
            .filter(|pair| pair[0].distance(pair[1]) > WIRE_POINT_EPSILON)
            .count();
        let mut segment_index = 0usize;

        for pair in points.windows(2) {
            let start = pair[0];
            let end = pair[1];
            if start.distance(end) <= WIRE_POINT_EPSILON {
                continue;
            }

            let seg_start_anchor = if segment_index == 0 {
                start_anchor
            } else {
                None
            };
            let seg_end_anchor = if segment_index + 1 == segment_count {
                end_anchor
            } else {
                None
            };
            self.add_wire_anchored(start, end, net, seg_start_anchor, seg_end_anchor);
            inserted += 1;
            segment_index += 1;
        }

        inserted
    }

    /// Split an existing wire at a junction point so future wires can connect to it.
    pub fn split_wire_at(&mut self, index: usize, point: Vec2) -> bool {
        let Some(wire) = self.wires.get(index).cloned() else {
            return false;
        };

        if point.distance(wire.start) <= WIRE_POINT_EPSILON
            || point.distance(wire.end) <= WIRE_POINT_EPSILON
        {
            return false;
        }

        self.wires.remove(index);
        self.add_wire_path_anchored(
            &[wire.start, point, wire.end],
            &wire.net,
            wire.start_anchor,
            wire.end_anchor,
        );
        true
    }

    /// Resolve all pin anchors into concrete positions. Call after component movement.
    pub fn sync_wire_anchors(&mut self) {
        let resolved = self
            .wires
            .iter()
            .map(|wire| {
                (
                    wire.start_anchor
                        .and_then(|anchor| self.resolve_anchor(anchor)),
                    wire.end_anchor
                        .and_then(|anchor| self.resolve_anchor(anchor)),
                )
            })
            .collect::<Vec<_>>();

        for (wire, (start, end)) in self.wires.iter_mut().zip(resolved) {
            if let Some(start) = start {
                wire.start = start;
            }
            if let Some(end) = end {
                wire.end = end;
            }
        }
    }

    pub fn resolve_anchor(&self, anchor: WireAnchor) -> Option<Vec2> {
        match anchor {
            WireAnchor::Point(point) => Some(point),
            WireAnchor::Pin {
                component_id,
                pin_id,
            } => self.pin_world_position_by_ids(component_id, pin_id),
        }
    }

    pub fn pin_world_position_by_ids(&self, component_id: Uuid, pin_id: Uuid) -> Option<Vec2> {
        let component = self
            .components
            .iter()
            .find(|component| component.id == component_id)?;
        let pin = component.pins.iter().find(|pin| pin.id == pin_id)?;
        Some(component_pin_world_position(component, pin))
    }

    /// Remove a wire by index.
    pub fn remove_wire(&mut self, index: usize) -> bool {
        if index < self.wires.len() {
            self.wires.remove(index);
            true
        } else {
            false
        }
    }

    /// Duplicate a component at the given index, offset slightly.
    pub fn duplicate_component(&mut self, index: usize) -> Option<Uuid> {
        if let Some(src) = self.components.get(index).cloned() {
            let mut dup = src;
            dup.id = Uuid::new_v4();
            // Offset so it does not overlap.
            dup.position += Vec2::new(40.0, 20.0);
            // Re-assign designator.
            let prefix = dup
                .designator
                .chars()
                .take_while(|c| c.is_alphabetic())
                .collect::<String>();
            let count = self
                .components
                .iter()
                .filter(|c| c.designator.starts_with(&prefix))
                .count();
            dup.designator = format!("{}{}", prefix, count + 1);
            // Give pins new IDs.
            for pin in &mut dup.pins {
                pin.id = Uuid::new_v4();
            }
            let id = dup.id;
            self.components.push(dup);
            Some(id)
        } else {
            None
        }
    }

    /// Run a full electrical / design rule check.
    /// Returns a list of warnings/errors as strings (backwards compatible).
    pub fn electrical_test(&self) -> Vec<String> {
        let report = crate::drc::run_drc(self);
        report.to_string_list()
    }

    /// Run a full DRC and return the structured report.
    pub fn run_drc(&self) -> crate::drc::DrcReport {
        crate::drc::run_drc(self)
    }

    /// Generate a netlist from this schematic.
    pub fn netlist(&self) -> crate::netlist::Netlist {
        crate::netlist::Netlist::from_schematic(self)
    }

    /// Run DC simulation on this schematic.
    pub fn simulate_dc(&self) -> crate::simulation::SimulationResults {
        crate::simulation::simulate_dc(self)
    }
}

pub fn component_pin_world_position(
    component: &ElectronicComponent,
    pin: &crate::component::Pin,
) -> Vec2 {
    let rot_rad = component.rotation.to_radians();
    let cos_r = rot_rad.cos();
    let sin_r = rot_rad.sin();
    let raw_ox = pin.offset.x * SCHEMATIC_GRID_STEP;
    let raw_oy = pin.offset.y * SCHEMATIC_GRID_STEP;
    let rot_ox = raw_ox * cos_r - raw_oy * sin_r;
    let rot_oy = raw_ox * sin_r + raw_oy * cos_r;
    Vec2::new(component.position.x + rot_ox, component.position.y + rot_oy)
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

    #[test]
    fn add_wire_path_skips_zero_length_segments() {
        let mut sch = Schematic::new("Test");
        let inserted = sch.add_wire_path(
            &[
                Vec2::new(0.0, 0.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(20.0, 0.0),
                Vec2::new(20.0, 20.0),
            ],
            "N001",
        );

        assert_eq!(inserted, 2);
        assert_eq!(sch.wires.len(), 2);
    }

    #[test]
    fn split_wire_creates_two_segments() {
        let mut sch = Schematic::new("Test");
        sch.add_wire(Vec2::new(0.0, 0.0), Vec2::new(40.0, 0.0), "N001");

        assert!(sch.split_wire_at(0, Vec2::new(20.0, 0.0)));
        assert_eq!(sch.wires.len(), 2);
        assert_eq!(sch.wires[0].start, Vec2::new(0.0, 0.0));
        assert_eq!(sch.wires[0].end, Vec2::new(20.0, 0.0));
        assert_eq!(sch.wires[1].start, Vec2::new(20.0, 0.0));
        assert_eq!(sch.wires[1].end, Vec2::new(40.0, 0.0));
    }

    #[test]
    fn anchored_wire_follows_moved_component_pin() {
        let mut sch = Schematic::new("Test");
        let mut resistor = ElectronicComponent::resistor("10k");
        resistor.position = Vec2::new(100.0, 100.0);
        let component_id = resistor.id;
        let pin_id = resistor.pins[0].id;
        sch.add_component(resistor);

        sch.add_wire_anchored(
            Vec2::new(80.0, 100.0),
            Vec2::new(40.0, 100.0),
            "N001",
            Some(WireAnchor::Pin {
                component_id,
                pin_id,
            }),
            Some(WireAnchor::Point(Vec2::new(40.0, 100.0))),
        );

        sch.components[0].position = Vec2::new(140.0, 100.0);
        sch.sync_wire_anchors();

        assert_eq!(sch.wires[0].start, Vec2::new(120.0, 100.0));
        assert_eq!(sch.wires[0].end, Vec2::new(40.0, 100.0));
    }

    #[test]
    fn split_wire_preserves_endpoint_anchors() {
        let mut sch = Schematic::new("Test");
        let start_anchor = Some(WireAnchor::Point(Vec2::new(0.0, 0.0)));
        let end_anchor = Some(WireAnchor::Point(Vec2::new(40.0, 0.0)));
        sch.add_wire_anchored(
            Vec2::new(0.0, 0.0),
            Vec2::new(40.0, 0.0),
            "N001",
            start_anchor,
            end_anchor,
        );

        assert!(sch.split_wire_at(0, Vec2::new(20.0, 0.0)));
        assert_eq!(sch.wires.len(), 2);
        assert_eq!(sch.wires[0].start_anchor, start_anchor);
        assert_eq!(sch.wires[0].end_anchor, None);
        assert_eq!(sch.wires[1].start_anchor, None);
        assert_eq!(sch.wires[1].end_anchor, end_anchor);
    }
}
