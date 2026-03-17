//! Netlist generation from schematic data.
//!
//! Traverses wires and component pins to build a list of electrical
//! nets (groups of connected pins). This is the foundation for
//! simulation, DRC, and export.

use crate::component::ElectronicComponent;
use crate::schematic::{Schematic, Wire};
use glam::Vec2;
use uuid::Uuid;

/// Tolerance for matching pin positions to wire endpoints (in grid units).
const POSITION_TOLERANCE: f32 = 2.0;

/// A single net: a named group of electrically connected pins.
#[derive(Debug, Clone)]
pub struct Net {
    /// Net identifier.
    pub id: usize,
    /// Net name (auto-generated or from wire label).
    pub name: String,
    /// Pins belonging to this net: (component_index, pin_index).
    pub pins: Vec<(usize, usize)>,
}

/// A component entry in the netlist.
#[derive(Debug, Clone)]
pub struct NetlistComponent {
    pub index: usize,
    pub id: Uuid,
    pub designator: String,
    pub value: String,
    pub footprint: String,
}

/// The complete netlist extracted from a schematic.
#[derive(Debug, Clone)]
pub struct Netlist {
    pub nets: Vec<Net>,
    pub components: Vec<NetlistComponent>,
}

impl Netlist {
    /// Build a netlist from a schematic.
    ///
    /// Algorithm:
    /// 1. Compute the world position of every pin.
    /// 2. For each wire, find which pins are within tolerance of each endpoint.
    /// 3. Use union-find to group pins connected through wires.
    /// 4. Assign net names (from wire labels or auto N001, N002, ...).
    pub fn from_schematic(schematic: &Schematic) -> Self {
        let components: Vec<NetlistComponent> = schematic
            .components
            .iter()
            .enumerate()
            .map(|(i, c)| NetlistComponent {
                index: i,
                id: c.id,
                designator: c.designator.clone(),
                value: c.value.clone(),
                footprint: c.footprint.clone(),
            })
            .collect();

        // Collect all pins with their world positions.
        // Each entry: (component_index, pin_index, world_position)
        let mut pin_positions: Vec<(usize, usize, Vec2)> = Vec::new();
        for (ci, comp) in schematic.components.iter().enumerate() {
            let rot_rad = comp.rotation.to_radians();
            let cos_r = rot_rad.cos();
            let sin_r = rot_rad.sin();
            for (pi, pin) in comp.pins.iter().enumerate() {
                let raw_ox = pin.offset.x * 20.0; // GRID_STEP
                let raw_oy = pin.offset.y * 20.0;
                let rot_ox = raw_ox * cos_r - raw_oy * sin_r;
                let rot_oy = raw_ox * sin_r + raw_oy * cos_r;
                let world = Vec2::new(
                    comp.position.x + rot_ox,
                    comp.position.y + rot_oy,
                );
                pin_positions.push((ci, pi, world));
            }
        }

        let pin_count = pin_positions.len();

        // Union-Find.
        let mut parent: Vec<usize> = (0..pin_count).collect();

        // Find root with path compression.
        fn find(parent: &mut Vec<usize>, mut x: usize) -> usize {
            while parent[x] != x {
                parent[x] = parent[parent[x]];
                x = parent[x];
            }
            x
        }

        // Union two sets.
        fn union(parent: &mut Vec<usize>, a: usize, b: usize) {
            let ra = find(parent, a);
            let rb = find(parent, b);
            if ra != rb {
                parent[rb] = ra;
            }
        }

        // For each wire, find pins near its endpoints and union them.
        for wire in &schematic.wires {
            let wire_start = Vec2::new(wire.start.x, wire.start.y);
            let wire_end = Vec2::new(wire.end.x, wire.end.y);

            let mut start_pins: Vec<usize> = Vec::new();
            let mut end_pins: Vec<usize> = Vec::new();

            for (idx, (_ci, _pi, pos)) in pin_positions.iter().enumerate() {
                if pos.distance(wire_start) < POSITION_TOLERANCE {
                    start_pins.push(idx);
                }
                if pos.distance(wire_end) < POSITION_TOLERANCE {
                    end_pins.push(idx);
                }
            }

            // Union all start-side pins together.
            for i in 1..start_pins.len() {
                union(&mut parent, start_pins[0], start_pins[i]);
            }
            // Union all end-side pins together.
            for i in 1..end_pins.len() {
                union(&mut parent, end_pins[0], end_pins[i]);
            }
            // Union start with end (the wire connects them).
            if !start_pins.is_empty() && !end_pins.is_empty() {
                union(&mut parent, start_pins[0], end_pins[0]);
            }
        }

        // Also union wires that share endpoints (wire junctions).
        // Represented as virtual pin indices starting after real pins.
        // Simpler approach: for each pair of wires sharing an endpoint,
        // find pins near each shared endpoint and union them.
        for i in 0..schematic.wires.len() {
            for j in (i + 1)..schematic.wires.len() {
                let wi = &schematic.wires[i];
                let wj = &schematic.wires[j];
                let points_i = [
                    Vec2::new(wi.start.x, wi.start.y),
                    Vec2::new(wi.end.x, wi.end.y),
                ];
                let points_j = [
                    Vec2::new(wj.start.x, wj.start.y),
                    Vec2::new(wj.end.x, wj.end.y),
                ];

                for pi in &points_i {
                    for pj in &points_j {
                        if pi.distance(*pj) < POSITION_TOLERANCE {
                            // These two wire endpoints meet; union any pins near them.
                            let mut nearby: Vec<usize> = Vec::new();
                            for (idx, (_ci, _pi, pos)) in pin_positions.iter().enumerate() {
                                if pos.distance(*pi) < POSITION_TOLERANCE {
                                    nearby.push(idx);
                                }
                            }
                            for k in 1..nearby.len() {
                                union(&mut parent, nearby[0], nearby[k]);
                            }
                        }
                    }
                }
            }
        }

        // Group pins by root into nets.
        let mut net_map: std::collections::HashMap<usize, Vec<(usize, usize)>> =
            std::collections::HashMap::new();

        for (idx, (ci, pi, _pos)) in pin_positions.iter().enumerate() {
            let root = find(&mut parent, idx);
            net_map.entry(root).or_default().push((*ci, *pi));
        }

        // Determine net names from wire labels.
        let mut net_names: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();

        for wire in &schematic.wires {
            if wire.net.is_empty() {
                continue;
            }
            let wire_start = Vec2::new(wire.start.x, wire.start.y);
            // Find any pin near this wire's start to get its root.
            for (idx, (_ci, _pi, pos)) in pin_positions.iter().enumerate() {
                if pos.distance(wire_start) < POSITION_TOLERANCE {
                    let root = find(&mut parent, idx);
                    net_names.entry(root).or_insert_with(|| wire.net.clone());
                    break;
                }
            }
        }

        // Build final net list.
        let mut nets: Vec<Net> = Vec::new();
        let mut auto_id = 1usize;
        let mut sorted_roots: Vec<usize> = net_map.keys().cloned().collect();
        sorted_roots.sort();

        for root in sorted_roots {
            let pins = net_map.remove(&root).unwrap_or_default();
            if pins.is_empty() {
                continue;
            }
            let name = if let Some(n) = net_names.get(&root) {
                n.clone()
            } else {
                let n = format!("N{:03}", auto_id);
                auto_id += 1;
                n
            };
            nets.push(Net {
                id: nets.len(),
                name,
                pins,
            });
        }

        Netlist { nets, components }
    }

    /// Find which net a specific pin belongs to.
    pub fn net_for_pin(&self, comp_index: usize, pin_index: usize) -> Option<&Net> {
        self.nets
            .iter()
            .find(|n| n.pins.iter().any(|&(ci, pi)| ci == comp_index && pi == pin_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;
    use crate::schematic::Schematic;

    #[test]
    fn empty_schematic_produces_empty_netlist() {
        let sch = Schematic::new("Test");
        let nl = Netlist::from_schematic(&sch);
        assert!(nl.nets.is_empty());
        assert!(nl.components.is_empty());
    }

    #[test]
    fn single_component_unconnected() {
        let mut sch = Schematic::new("Test");
        let mut r = ElectronicComponent::resistor("10k");
        r.position = Vec2::new(100.0, 100.0);
        sch.add_component(r);
        let nl = Netlist::from_schematic(&sch);
        // Each pin in its own net (unconnected).
        assert_eq!(nl.components.len(), 1);
        // Two pins, each in separate net.
        assert_eq!(nl.nets.len(), 2);
    }

    #[test]
    fn two_components_connected_by_wire() {
        let mut sch = Schematic::new("Test");
        let mut r1 = ElectronicComponent::resistor("10k");
        r1.position = Vec2::new(100.0, 100.0);
        sch.add_component(r1);

        let mut r2 = ElectronicComponent::resistor("4.7k");
        r2.position = Vec2::new(140.0, 100.0);
        sch.add_component(r2);

        // Wire from R1 pin 2 (offset +20) to R2 pin 1 (offset -20).
        // R1 at 100, pin2 offset = +1*20 = at x=120.
        // R2 at 140, pin1 offset = -1*20 = at x=120.
        sch.add_wire(
            Vec2::new(120.0, 100.0),
            Vec2::new(120.0, 100.0),
            "VCC",
        );

        let nl = Netlist::from_schematic(&sch);
        // R1.pin2 and R2.pin1 should be in the same net.
        let net = nl.net_for_pin(0, 1); // R1 pin 2
        assert!(net.is_some());
        let net = net.unwrap();
        assert_eq!(net.name, "VCC");
        // That net should also contain R2 pin 1.
        assert!(net.pins.iter().any(|&(ci, pi)| ci == 1 && pi == 0));
    }
}
