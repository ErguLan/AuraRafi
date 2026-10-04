//! Netlist generation from schematic data.
//!
//! Traverses wires and component pins to build a list of electrical
//! nets (groups of connected pins). This is the foundation for
//! simulation, DRC, and export.

use crate::schematic::{Schematic, Wire};
use glam::Vec2;
use std::collections::HashMap;
use uuid::Uuid;
const _GRID_STEP: f32 = 20.0;
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
    /// 1. Compute the world position of every pin once.
    /// 2. For each wire, find which pins are within tolerance of each endpoint.
    /// 3. Use union-find to group pins connected through wires.
    /// 4. Assign net names (from wire labels or auto N001, N002, ...).
    ///
    /// Pin lookups go through a spatial hash instead of scanning every pin, and
    /// wire endpoints are resolved once instead of once per wire pair, so the
    /// junction pass no longer costs O(wires^2 * pins).
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
        // Each entry: (component_index, pin_index) paired with its world position.
        let mut pin_index: Vec<(usize, usize)> = Vec::new();
        let mut pin_positions: Vec<Vec2> = Vec::new();
        for (ci, comp) in schematic.components.iter().enumerate() {
            for (pi, pin) in comp.pins.iter().enumerate() {
                pin_index.push((ci, pi));
                pin_positions.push(crate::schematic::component_pin_world_position(comp, pin));
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

        // Wire endpoints are resolved once here. Anchor resolution walks the
        // component list, so repeating it inside the wire-pair loop was the
        // dominant cost of netlist extraction on dense schematics.
        let grid = PinGrid::new(&pin_positions);
        let wire_endpoints: Vec<[Vec2; 2]> = schematic
            .wires
            .iter()
            .map(|wire| {
                [
                    resolve_wire_point(schematic, wire, true),
                    resolve_wire_point(schematic, wire, false),
                ]
            })
            .collect();

        // For each wire, find pins near its endpoints and union them.
        for endpoints in &wire_endpoints {
            let start_pins = grid.near(endpoints[0]);
            let end_pins = grid.near(endpoints[1]);

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
        // For each pair of wires sharing an endpoint, union any pins near them.
        // Nothing can share an endpoint with a single wire, so skip the pass.
        if wire_endpoints.len() >= 2 {
            for i in 0..wire_endpoints.len() {
                for j in (i + 1)..wire_endpoints.len() {
                    for pi in &wire_endpoints[i] {
                        let shares_endpoint = wire_endpoints[j]
                            .iter()
                            .any(|pj| pi.distance(*pj) < POSITION_TOLERANCE);
                        if !shares_endpoint {
                            continue;
                        }
                        // These two wire endpoints meet; union any pins near them.
                        let nearby = grid.near(*pi);
                        for k in 1..nearby.len() {
                            union(&mut parent, nearby[0], nearby[k]);
                        }
                    }
                }
            }
        }

        // Group pins by root into nets.
        let mut net_map: std::collections::HashMap<usize, Vec<(usize, usize)>> =
            std::collections::HashMap::new();

        for (idx, key) in pin_index.iter().enumerate() {
            let root = find(&mut parent, idx);
            net_map.entry(root).or_default().push(*key);
        }

        // Determine net names from wire labels.
        let mut net_names: std::collections::HashMap<usize, String> =
            std::collections::HashMap::new();

        for (index, wire) in schematic.wires.iter().enumerate() {
            if wire.net.is_empty() {
                continue;
            }
            let wire_start = wire_endpoints[index][0];
            // Find the first pin near this wire's start to get its root.
            if let Some(&pin) = grid.near(wire_start).first() {
                let root = find(&mut parent, pin);
                net_names.entry(root).or_insert_with(|| wire.net.clone());
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
        self.nets.iter().find(|n| {
            n.pins
                .iter()
                .any(|&(ci, pi)| ci == comp_index && pi == pin_index)
        })
    }
}

/// Resolves a wire endpoint, preferring the persisted anchor when it is set.
fn resolve_wire_point(schematic: &Schematic, wire: &Wire, start: bool) -> Vec2 {
    let (anchor, point) = if start {
        (wire.start_anchor, wire.start)
    } else {
        (wire.end_anchor, wire.end)
    };
    match anchor.and_then(|anchor| schematic.resolve_anchor(anchor)) {
        Some(resolved) => resolved,
        None => point,
    }
}

/// Uniform spatial hash over pin world positions.
///
/// The junction pass asks for "every pin within `POSITION_TOLERANCE` of this
/// point" once per wire pair, which made extraction O(wires^2 * pins). Bucketing
/// the pins once makes each lookup cheap without changing which pins match: the
/// cell size equals the match radius, so any pin closer than the radius is always
/// inside the 3x3 cell block around the query point.
struct PinGrid {
    cell_size: f32,
    points: Vec<Vec2>,
    buckets: HashMap<(i64, i64), Vec<usize>>,
}

impl PinGrid {
    fn new(points: &[Vec2]) -> Self {
        let cell_size = POSITION_TOLERANCE;
        let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (index, point) in points.iter().enumerate() {
            buckets
                .entry(Self::cell_of(*point, cell_size))
                .or_default()
                .push(index);
        }

        Self {
            cell_size,
            points: points.to_vec(),
            buckets,
        }
    }

    fn cell_of(point: Vec2, cell_size: f32) -> (i64, i64) {
        (
            (point.x / cell_size).floor() as i64,
            (point.y / cell_size).floor() as i64,
        )
    }

    /// Indices of the pins strictly within `POSITION_TOLERANCE` of `point`,
    /// ascending so callers see a stable order.
    fn near(&self, point: Vec2) -> Vec<usize> {
        let (cell_x, cell_y) = Self::cell_of(point, self.cell_size);
        let mut matches = Vec::new();

        for offset_x in -1..=1 {
            for offset_y in -1..=1 {
                let Some(bucket) = self.buckets.get(&(cell_x + offset_x, cell_y + offset_y)) else {
                    continue;
                };
                for &index in bucket {
                    if self.points[index].distance(point) < POSITION_TOLERANCE {
                        matches.push(index);
                    }
                }
            }
        }

        matches.sort_unstable();
        matches
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;
    use crate::schematic::{Schematic, WireAnchor};

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
        sch.add_wire(Vec2::new(120.0, 100.0), Vec2::new(120.0, 100.0), "VCC");

        let nl = Netlist::from_schematic(&sch);
        // R1.pin2 and R2.pin1 should be in the same net.
        let net = nl.net_for_pin(0, 1); // R1 pin 2
        assert!(net.is_some());
        let net = net.unwrap();
        assert_eq!(net.name, "VCC");
        // That net should also contain R2 pin 1.
        assert!(net.pins.iter().any(|&(ci, pi)| ci == 1 && pi == 0));
    }

    #[test]
    fn anchored_wire_endpoints_drive_netlist_even_when_points_are_stale() {
        let mut sch = Schematic::new("Test");
        let mut r1 = ElectronicComponent::resistor("10k");
        r1.position = Vec2::new(100.0, 100.0);
        let r1_id = r1.id;
        let r1_pin2 = r1.pins[1].id;
        sch.add_component(r1);

        let mut r2 = ElectronicComponent::resistor("4.7k");
        r2.position = Vec2::new(200.0, 100.0);
        let r2_id = r2.id;
        let r2_pin1 = r2.pins[0].id;
        sch.add_component(r2);

        sch.add_wire_anchored(
            Vec2::ZERO,
            Vec2::ZERO,
            "NET_A",
            Some(WireAnchor::Pin {
                component_id: r1_id,
                pin_id: r1_pin2,
            }),
            Some(WireAnchor::Pin {
                component_id: r2_id,
                pin_id: r2_pin1,
            }),
        );

        let nl = Netlist::from_schematic(&sch);
        let net = nl.net_for_pin(0, 1).expect("R1 pin 2 should be connected");
        assert_eq!(net.name, "NET_A");
        assert!(net.pins.iter().any(|&(ci, pi)| ci == 1 && pi == 0));
    }

    #[test]
    fn pin_grid_matches_a_brute_force_tolerance_scan() {
        let points = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.9, 0.0),
            Vec2::new(2.1, 0.0),
            Vec2::new(-1.5, 1.5),
            Vec2::new(40.0, -20.0),
            Vec2::new(40.0, -20.5),
        ];
        let grid = PinGrid::new(&points);
        let probes = [
            Vec2::new(0.0, 0.0),
            Vec2::new(2.0, 0.0),
            Vec2::new(-2.0, 2.0),
            Vec2::new(40.0, -20.0),
            Vec2::new(100.0, 100.0),
        ];

        for probe in probes {
            let mut expected = Vec::new();
            for (index, point) in points.iter().enumerate() {
                if point.distance(probe) < POSITION_TOLERANCE {
                    expected.push(index);
                }
            }
            assert_eq!(grid.near(probe), expected, "probe {probe:?}");
        }
    }

    #[test]
    fn pins_inside_the_match_tolerance_share_a_net() {
        let mut sch = Schematic::new("Tolerance");
        let mut r1 = ElectronicComponent::resistor("10k");
        r1.position = Vec2::new(100.0, 100.0);
        sch.add_component(r1);
        let mut r2 = ElectronicComponent::resistor("4.7k");
        r2.position = Vec2::new(140.0, 100.0);
        sch.add_component(r2);

        // The endpoint is deliberately off the shared pin location but still
        // inside POSITION_TOLERANCE.
        sch.add_wire(Vec2::new(120.0, 100.0), Vec2::new(120.0, 101.5), "VCC");

        let nl = Netlist::from_schematic(&sch);
        let net = nl.net_for_pin(0, 1).expect("R1 pin 2 should be connected");
        assert_eq!(net.name, "VCC");
        assert!(net.pins.iter().any(|&(ci, pi)| ci == 1 && pi == 0));
    }
}
