use super::geometry::{copper_items, CONTACT_EPSILON};
use std::collections::{BTreeMap, BTreeSet, HashMap};

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::pcb::footprint::footprint_definition;
use crate::schematic::Schematic;

const BOARD_EDGE_EPSILON: f32 = 0.5;

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
    /// Stable pin identities keep routing attached to the same electrical
    /// network when automatic names or component ordering change.
    #[serde(default)]
    pub net_pins: BTreeMap<String, Vec<Uuid>>,
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
            net_pins: BTreeMap::new(),
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

        let mut old_net_pins = self.net_pins.clone();
        if old_net_pins.is_empty() {
            // Upgrade old documents without throwing away their routing. A
            // missing old component supplies an unmatched identity, so those
            // traces are reviewed rather than assigned to a recycled net name.
            for placement in &self.components {
                for (pi, name) in placement.pad_nets.iter().enumerate() {
                    let pin_id = schematic
                        .components
                        .iter()
                        .find(|c| c.id == placement.component_id)
                        .and_then(|c| c.pins.get(pi))
                        .map(|p| p.id)
                        .unwrap_or(placement.component_id);
                    old_net_pins.entry(name.clone()).or_default().push(pin_id);
                }
            }
        }
        let pin_names: HashMap<_, _> = netlist
            .nets
            .iter()
            .flat_map(|net| {
                net.pins
                    .iter()
                    .map(move |&(ci, pi)| (schematic.components[ci].pins[pi].id, net.name.clone()))
            })
            .collect();
        self.net_pins = netlist
            .nets
            .iter()
            .map(|net| {
                let mut ids: Vec<_> = net
                    .pins
                    .iter()
                    .map(|&(ci, pi)| schematic.components[ci].pins[pi].id)
                    .collect();
                ids.sort();
                (net.name.clone(), ids)
            })
            .collect();
        let renamed: HashMap<String, Option<String>> = old_net_pins
            .iter()
            .map(|(old_name, pins)| {
                let candidate = pins.first().and_then(|pin| pin_names.get(pin)).cloned();
                let same_network = candidate
                    .as_ref()
                    .is_some_and(|name| pins.iter().all(|pin| pin_names.get(pin) == Some(name)));
                (
                    old_name.clone(),
                    if same_network { candidate } else { None },
                )
            })
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

        summary.removed_components = existing
            .keys()
            .filter(|id| !schematic.components.iter().any(|c| c.id == **id))
            .count();
        summary.nets = net_names.len();

        self.components = synced;
        // Traces whose net no longer exists are not deleted here. Dropping them
        // silently destroyed user routing that the editor could neither report
        // nor offer back, so they are parked in `removed_traces` for the UI to
        // review and let the user re-apply or discard explicitly.
        let mut orphaned = Vec::new();
        let mut retained = Vec::with_capacity(self.traces.len());
        for mut trace in self.traces.drain(..) {
            let remapped = renamed.get(&trace.net);
            if let Some(Some(name)) = remapped {
                trace.net = name.clone();
            }
            if matches!(remapped, Some(Some(_))) && net_names.contains(&trace.net) {
                retained.push(trace);
            } else {
                orphaned.push(trace);
            }
        }
        self.traces = retained;
        for trace in orphaned {
            if !self.removed_traces.iter().any(|old| old.id == trace.id) {
                self.removed_traces.push(trace);
            }
        }
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
        self.try_route_airwire(index).is_ok()
    }

    pub fn rebuild_airwires(&mut self) {
        self.rebuild_airwires_cancellable(&std::sync::atomic::AtomicBool::new(false));
    }

    pub(crate) fn rebuild_airwires_cancellable(
        &mut self,
        cancel: &std::sync::atomic::AtomicBool,
    ) -> bool {
        self.airwires.clear();
        let items = copper_items(self);
        let mut dsu = DisjointSet::new(items.len());
        super::geometry::nearby_pairs_cancellable(&items, CONTACT_EPSILON, cancel, |a, b| {
            if !items[a].net.is_empty()
                && items[a].net == items[b].net
                && items[a].shape.distance(&items[b].shape) <= CONTACT_EPSILON
            {
                dsu.union(a, b);
            }
        });
        if cancel.load(std::sync::atomic::Ordering::Acquire) {
            return false;
        }
        let mut nets = BTreeMap::<String, BTreeMap<usize, Vec<(Uuid, Vec2)>>>::new();
        for (index, item) in items.iter().enumerate() {
            let Some((ci, pi)) = item.pad else {
                continue;
            };
            if item.net.is_empty() {
                continue;
            }
            if let Some(position) = self.pad_world_position(ci, pi) {
                nets.entry(item.net.clone())
                    .or_default()
                    .entry(dsu.find(index))
                    .or_default()
                    .push((item.source, position));
            }
        }
        for (net, groups) in nets {
            let mut groups: Vec<_> = groups.into_values().collect();
            if groups.len() < 2 {
                continue;
            }
            // Connect each island to its closest already-connected island. This
            // keeps the ratsnest deterministic without an arbitrary long star.
            let mut connected = groups.remove(0);
            while !groups.is_empty() {
                let mut best = (f32::INFINITY, 0, connected[0], groups[0][0]);
                for (gi, group) in groups.iter().enumerate() {
                    for &from in &connected {
                        for &to in group {
                            if cancel.load(std::sync::atomic::Ordering::Acquire) {
                                return false;
                            }
                            let distance = from.1.distance_squared(to.1);
                            if distance < best.0 {
                                best = (distance, gi, from, to);
                            }
                        }
                    }
                }
                self.airwires.push(PcbAirwire {
                    net: net.clone(),
                    from_component_id: best.2 .0,
                    from: best.2 .1,
                    to_component_id: best.3 .0,
                    to: best.3 .1,
                });
                connected.extend(groups.remove(best.1));
            }
        }
        true
    }
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

    fn find(&mut self, mut node: usize) -> usize {
        while self.parent[node] != node {
            self.parent[node] = self.parent[self.parent[node]];
            node = self.parent[node];
        }
        node
    }

    fn union(&mut self, a: usize, b: usize) {
        let root_a = self.find(a);
        let root_b = self.find(b);
        if root_a != root_b {
            self.parent[root_a.max(root_b)] = root_a.min(root_b);
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
}
