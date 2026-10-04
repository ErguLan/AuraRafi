use std::collections::{BTreeSet, HashMap};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pcb::footprint::footprint_definition;
use crate::schematic::Schematic;

const BOARD_EDGE_EPSILON: f32 = 0.5;
const CONNECT_EPSILON: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PcbLayer {
    TopCopper,
    BottomCopper,
}

impl PcbLayer {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TopCopper => "Top Copper",
            Self::BottomCopper => "Bottom Copper",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoardOutline {
    pub points: Vec<Vec2>,
}

impl BoardOutline {
    pub fn default_rect(width: f32, height: f32) -> Self {
        Self {
            points: vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(width, 0.0),
                Vec2::new(width, height),
                Vec2::new(0.0, height),
                Vec2::new(0.0, 0.0),
            ],
        }
    }

    pub fn is_closed(&self) -> bool {
        if self.points.len() < 4 {
            return false;
        }

        self.points
            .first()
            .zip(self.points.last())
            .map(|(first, last)| first.distance(*last) <= BOARD_EDGE_EPSILON)
            .unwrap_or(false)
    }

    pub fn bounds(&self) -> Option<(Vec2, Vec2)> {
        let first = *self.points.first()?;
        let mut min = first;
        let mut max = first;

        for point in &self.points[1..] {
            min.x = min.x.min(point.x);
            min.y = min.y.min(point.y);
            max.x = max.x.max(point.x);
            max.y = max.y.max(point.y);
        }

        Some((min, max))
    }

    pub fn size(&self) -> Vec2 {
        self.bounds()
            .map(|(min, max)| max - min)
            .unwrap_or(Vec2::ZERO)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbComponentPlacement {
    pub component_id: Uuid,
    pub designator: String,
    pub value: String,
    pub footprint: String,
    pub position: Vec2,
    pub rotation: f32,
    pub layer: PcbLayer,
    pub locked: bool,
    pub image_asset: Option<String>,
    pub pad_nets: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbTrace {
    pub id: Uuid,
    pub net: String,
    pub layer: PcbLayer,
    pub width: f32,
    pub points: Vec<Vec2>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbAirwire {
    pub net: String,
    pub from_component_id: Uuid,
    pub from: Vec2,
    pub to_component_id: Uuid,
    pub to: Vec2,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbLayout {
    pub name: String,
    pub board_outline: BoardOutline,
    pub components: Vec<PcbComponentPlacement>,
    pub traces: Vec<PcbTrace>,
    pub airwires: Vec<PcbAirwire>,
    /// Traces orphaned by the last schematic sync; retained for explicit review
    /// instead of being deleted without notice.
    #[serde(default)]
    pub removed_traces: Vec<PcbTrace>,
}

#[derive(Debug, Clone, Default)]
pub struct PcbSyncSummary {
    pub added_components: usize,
    pub updated_components: usize,
    pub removed_components: usize,
    pub nets: usize,
}

impl PcbLayout {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            board_outline: BoardOutline::default_rect(420.0, 280.0),
            components: Vec::new(),
            traces: Vec::new(),
            airwires: Vec::new(),
            removed_traces: Vec::new(),
        }
    }

    pub fn board_size(&self) -> Vec2 {
        self.board_outline.size()
    }

    pub fn outline_is_closed(&self) -> bool {
        self.board_outline.is_closed()
    }

    pub fn missing_footprints(&self) -> usize {
        self.components
            .iter()
            .filter(|component| component.footprint.trim().is_empty())
            .count()
    }

    pub fn sync_from_schematic(&mut self, schematic: &Schematic) -> PcbSyncSummary {
        self.name = schematic.name.clone();

        let netlist = schematic.netlist();
        let mut net_names = BTreeSet::new();
        for net in &netlist.nets {
            net_names.insert(net.name.clone());
        }

        let existing: HashMap<Uuid, PcbComponentPlacement> = self
            .components
            .iter()
            .cloned()
            .map(|placement| (placement.component_id, placement))
            .collect();

        let mut summary = PcbSyncSummary::default();
        let mut synced = Vec::with_capacity(schematic.components.len());

        for (index, component) in schematic.components.iter().enumerate() {
            let pad_nets = component
                .pins
                .iter()
                .enumerate()
                .map(|(pin_idx, _)| {
                    netlist
                        .net_for_pin(index, pin_idx)
                        .map(|net| net.name.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>();

            if let Some(existing_component) = existing.get(&component.id) {
                summary.updated_components += 1;
                synced.push(PcbComponentPlacement {
                    component_id: component.id,
                    designator: component.designator.clone(),
                    value: component.value.clone(),
                    footprint: component.footprint.clone(),
                    position: existing_component.position,
                    rotation: existing_component.rotation,
                    layer: existing_component.layer,
                    locked: existing_component.locked,
                    image_asset: existing_component.image_asset.clone(),
                    pad_nets,
                });
            } else {
                summary.added_components += 1;
                synced.push(PcbComponentPlacement {
                    component_id: component.id,
                    designator: component.designator.clone(),
                    value: component.value.clone(),
                    footprint: component.footprint.clone(),
                    position: Vec2::new(
                        70.0 + (index % 4) as f32 * 82.0,
                        70.0 + (index / 4) as f32 * 64.0,
                    ),
                    rotation: component.rotation,
                    layer: PcbLayer::TopCopper,
                    locked: false,
                    image_asset: footprint_definition(&component.footprint, component.pins.len())
                        .preview_asset,
                    pad_nets,
                });
            }
        }

        summary.removed_components = existing.len().saturating_sub(schematic.components.len());
        summary.nets = net_names.len();

        self.components = synced;
        // Traces whose net no longer exists are not deleted here. Dropping them
        // silently destroyed user routing that the editor could neither report
        // nor offer back, so they are parked in `removed_traces` for the UI to
        // review and let the user re-apply or discard explicitly.
        let mut orphaned = Vec::new();
        let mut retained = Vec::with_capacity(self.traces.len());
        for trace in self.traces.drain(..) {
            if net_names.contains(&trace.net) {
                retained.push(trace);
            } else {
                orphaned.push(trace);
            }
        }
        self.traces = retained;
        self.removed_traces = orphaned;
        self.rebuild_airwires();
        summary
    }

    pub fn pad_world_position(&self, component_index: usize, pad_index: usize) -> Option<Vec2> {
        let component = self.components.get(component_index)?;
        let footprint = footprint_definition(&component.footprint, component.pad_nets.len().max(1));
        let pad = footprint.pads.get(pad_index)?;
        let radians = component.rotation.to_radians();
        let sin = radians.sin();
        let cos = radians.cos();
        let rotated = Vec2::new(
            pad.offset.x * cos - pad.offset.y * sin,
            pad.offset.x * sin + pad.offset.y * cos,
        );
        Some(component.position + rotated)
    }

    pub fn delete_trace(&mut self, index: usize) -> bool {
        if index >= self.traces.len() {
            return false;
        }

        self.traces.remove(index);
        self.rebuild_airwires();
        true
    }

    pub fn route_airwire(&mut self, index: usize) -> bool {
        let Some(airwire) = self.airwires.get(index).cloned() else {
            return false;
        };

        let mut points = vec![airwire.from];
        let corner = Vec2::new(airwire.to.x, airwire.from.y);
        if airwire.from.distance(corner) > 0.1 && airwire.to.distance(corner) > 0.1 {
            points.push(corner);
        }
        points.push(airwire.to);

        self.traces.push(PcbTrace {
            id: Uuid::new_v4(),
            net: airwire.net,
            layer: PcbLayer::TopCopper,
            width: 6.0,
            points,
        });

        self.rebuild_airwires();
        true
    }

    pub fn rebuild_airwires(&mut self) {
        self.airwires.clear();

        let mut net_to_pads: HashMap<String, Vec<(Uuid, Vec2)>> = HashMap::new();
        for (component_index, component) in self.components.iter().enumerate() {
            for (pad_index, net_name) in component.pad_nets.iter().enumerate() {
                if net_name.trim().is_empty() {
                    continue;
                }

                if let Some(position) = self.pad_world_position(component_index, pad_index) {
                    net_to_pads
                        .entry(net_name.clone())
                        .or_default()
                        .push((component.component_id, position));
                }
            }
        }

        for (net_name, pads) in net_to_pads {
            if pads.len() < 2 {
                continue;
            }

            let mut nodes: Vec<Vec2> = pads.iter().map(|(_, point)| *point).collect();
            let pad_node_count = nodes.len();
            let trace_ranges = self
                .traces
                .iter()
                .filter(|trace| trace.net == net_name)
                .map(|trace| {
                    let start = nodes.len();
                    nodes.extend(trace.points.iter().copied());
                    (start, nodes.len())
                })
                .collect::<Vec<_>>();

            let mut dsu = DisjointSet::new(nodes.len());

            for (start, end) in trace_ranges {
                for node_index in start..end.saturating_sub(1) {
                    dsu.union(node_index, node_index + 1);
                }
            }

            connect_close_nodes(&nodes, &mut dsu);

            let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
            for pad_index in 0..pad_node_count {
                groups
                    .entry(dsu.find(pad_index))
                    .or_default()
                    .push(pad_index);
            }

            let mut roots = groups.into_values().collect::<Vec<_>>();
            if roots.len() <= 1 {
                continue;
            }

            roots.sort_by_key(|group| group[0]);
            let anchor_index = roots[0][0];
            for group in roots.iter().skip(1) {
                let target_index = group[0];
                self.airwires.push(PcbAirwire {
                    net: net_name.clone(),
                    from_component_id: pads[anchor_index].0,
                    from: pads[anchor_index].1,
                    to_component_id: pads[target_index].0,
                    to: pads[target_index].1,
                });
            }
        }
    }
}

/// Buckets node indices by position rounded to `CONNECT_EPSILON`.
///
/// `rebuild_airwires` used to test every pair of nodes of a net, which is
/// quadratic and dominates the sync once a net carries routed traces. Two nodes
/// within `CONNECT_EPSILON` can differ by at most one cell on each axis, so
/// scanning the 3x3 cell block finds exactly the same pairs the quadratic sweep
/// did while visiting each candidate a constant number of times.
fn connect_close_nodes(nodes: &[Vec2], dsu: &mut DisjointSet) {
    let mut buckets: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (index, node) in nodes.iter().enumerate() {
        buckets.entry(node_cell(*node)).or_default().push(index);
    }

    for (index, node) in nodes.iter().enumerate() {
        let (cell_x, cell_y) = node_cell(*node);
        for offset_x in -1..=1 {
            for offset_y in -1..=1 {
                let Some(bucket) = buckets.get(&(cell_x + offset_x, cell_y + offset_y)) else {
                    continue;
                };
                for &other in bucket {
                    // Only look forward so every close pair is visited once.
                    if other <= index {
                        continue;
                    }
                    if node.distance(nodes[other]) <= CONNECT_EPSILON {
                        dsu.union(index, other);
                    }
                }
            }
        }
    }
}

fn node_cell(node: Vec2) -> (i64, i64) {
    (
        (node.x / CONNECT_EPSILON).floor() as i64,
        (node.y / CONNECT_EPSILON).floor() as i64,
    )
}

struct DisjointSet {
    parent: Vec<usize>,
}

impl DisjointSet {
    fn new(size: usize) -> Self {
        Self {
            parent: (0..size).collect(),
        }
    }

    fn find(&mut self, node: usize) -> usize {
        if self.parent[node] != node {
            let root = self.find(self.parent[node]);
            self.parent[node] = root;
        }
        self.parent[node]
    }

    fn union(&mut self, a: usize, b: usize) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            self.parent[root_b] = root_a;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;

    #[test]
    fn default_outline_is_closed() {
        let layout = PcbLayout::new("Board");
        assert!(layout.outline_is_closed());
    }

    #[test]
    fn sync_preserves_manual_position() {
        let mut schematic = Schematic::new("Board");
        let component_id = schematic.add_component(ElectronicComponent::resistor("10k"));

        let mut layout = PcbLayout::new("Board");
        let _ = layout.sync_from_schematic(&schematic);
        layout.components[0].position = Vec2::new(210.0, 90.0);

        let _ = layout.sync_from_schematic(&schematic);

        assert_eq!(layout.components[0].component_id, component_id);
        assert_eq!(layout.components[0].position, Vec2::new(210.0, 90.0));
    }

    fn trace_for(net: &str) -> PcbTrace {
        PcbTrace {
            id: Uuid::new_v4(),
            net: net.to_string(),
            layer: PcbLayer::TopCopper,
            width: 6.0,
            points: vec![Vec2::new(10.0, 10.0), Vec2::new(20.0, 10.0)],
        }
    }

    #[test]
    fn sync_keeps_traces_whose_net_disappeared_and_reports_them() {
        let mut schematic = Schematic::new("Board");
        schematic.add_component(ElectronicComponent::resistor("10k"));

        let mut layout = PcbLayout::new("Board");
        let _ = layout.sync_from_schematic(&schematic);
        let live_net = layout.components[0].pad_nets[0].clone();
        let orphaned = trace_for("GONE_NET");
        layout.traces.push(trace_for(&live_net));
        layout.traces.push(orphaned.clone());

        let _ = layout.sync_from_schematic(&schematic);

        assert!(
            layout.traces.iter().all(|trace| trace.net != "GONE_NET"),
            "traces must not be deleted by the sync"
        );
        assert!(layout.traces.iter().any(|trace| trace.net == live_net));
        assert_eq!(layout.removed_traces, vec![orphaned]);
    }

    /// Maps every node to the lowest index of its group so two disjoint sets can
    /// be compared by partition instead of by the arbitrary root they picked.
    fn canonical_groups(dsu: &mut DisjointSet, count: usize) -> Vec<usize> {
        let roots: Vec<usize> = (0..count).map(|index| dsu.find(index)).collect();
        (0..count)
            .map(|index| {
                let root = roots[index];
                roots
                    .iter()
                    .position(|other| *other == root)
                    .expect("root belongs to at least one node")
            })
            .collect()
    }

    #[test]
    fn close_node_search_matches_a_quadratic_sweep() {
        let nodes = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(6.0, 0.0),
            Vec2::new(0.0, 5.9),
            Vec2::new(6.1, 0.0),
            Vec2::new(40.0, 40.0),
            Vec2::new(40.0, 46.0),
        ];

        let mut grid_set = DisjointSet::new(nodes.len());
        connect_close_nodes(&nodes, &mut grid_set);

        let mut sweep_set = DisjointSet::new(nodes.len());
        for a in 0..nodes.len() {
            for b in (a + 1)..nodes.len() {
                if nodes[a].distance(nodes[b]) <= CONNECT_EPSILON {
                    sweep_set.union(a, b);
                }
            }
        }

        assert_eq!(
            canonical_groups(&mut grid_set, nodes.len()),
            canonical_groups(&mut sweep_set, nodes.len())
        );
    }
}
