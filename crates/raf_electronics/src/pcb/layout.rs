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

/// Plated through via joining the two supported copper faces, in millimetres.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbVia {
    pub id: Uuid,
    pub net: String,
    pub position: Vec2,
    pub diameter: f32,
    pub drill: f32,
    #[serde(default)]
    pub orphaned: bool,
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
    #[serde(default)]
    pub vias: Vec<PcbVia>,
    #[serde(default)]
    pub rules: super::rules::PcbDesignRules,
    pub airwires: Vec<PcbAirwire>,
    /// Traces orphaned by the last schematic sync; retained for explicit review
    /// instead of being deleted without notice.
    #[serde(default)]
    pub removed_traces: Vec<PcbTrace>,
    /// Stable pin identities keep routing attached to the same electrical
    /// network when automatic names or component ordering change.
    #[serde(default)]
    pub net_pins: BTreeMap<String, Vec<Uuid>>,
    /// Project-owned physical definitions; snapshots remain portable offline.
    #[serde(default)]
    pub footprint_assets: BTreeMap<String, super::footprint_asset::FootprintAsset>,
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
            vias: Vec::new(),
            rules: Default::default(),
            airwires: Vec::new(),
            removed_traces: Vec::new(),
            net_pins: BTreeMap::new(),
            footprint_assets: BTreeMap::new(),
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

    fn component_pad_nets(
        &self,
        index: usize,
        component: &crate::component::ElectronicComponent,
        netlist: &crate::netlist::Netlist,
    ) -> Vec<String> {
        if let Some(asset) = self.footprint_assets.get(&component.footprint) {
            asset
                .pads
                .iter()
                .map(|pad| {
                    if matches!(
                        &pad.technology,
                        super::footprint_asset::FootprintPadTechnology::NonPlatedThroughHole
                    ) || pad.number.trim().is_empty()
                    {
                        return String::new();
                    }
                    component
                        .pins
                        .iter()
                        .enumerate()
                        .find(|(_, pin)| {
                            if component.footprint_pin_map.is_empty() {
                                pin.name.trim() == pad.number.trim()
                            } else {
                                component
                                    .footprint_pin_map
                                    .get(&pin.name)
                                    .is_some_and(|number| number.trim() == pad.number.trim())
                            }
                        })
                        .and_then(|(pin_idx, _)| netlist.net_for_pin(index, pin_idx))
                        .map(|net| net.name.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        } else {
            component
                .pins
                .iter()
                .enumerate()
                .map(|(pin_idx, _)| {
                    netlist
                        .net_for_pin(index, pin_idx)
                        .map(|net| net.name.clone())
                        .unwrap_or_default()
                })
                .collect::<Vec<_>>()
        }
    }

    /// Compare schematic metadata and pad mapping; placement stays PCB-owned.
    pub fn matches_schematic(&self, schematic: &Schematic) -> bool {
        if self.components.len() != schematic.components.len()
            || self
                .components
                .iter()
                .map(|c| c.component_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.components.len()
        {
            return false;
        }
        let netlist = schematic.netlist();
        self.components.iter().all(|placement| {
            let Some((index, component)) = schematic
                .components
                .iter()
                .enumerate()
                .find(|(_, c)| c.id == placement.component_id)
            else {
                return false;
            };
            placement.designator == component.designator
                && placement.value == component.value
                && placement.footprint == component.footprint
                && placement.pad_nets == self.component_pad_nets(index, component, &netlist)
        })
    }

    fn placement_bounds(
        &self,
        footprint: &str,
        pins: usize,
        position: Vec2,
        rotation: f32,
    ) -> (Vec2, Vec2) {
        let (min, max) = if let Some(bounds) = self
            .footprint_assets
            .get(footprint)
            .and_then(|asset| asset.bounds_mm())
        {
            (
                Vec2::new(bounds.min.x as f32, bounds.min.y as f32),
                Vec2::new(bounds.max.x as f32, bounds.max.y as f32),
            )
        } else {
            let definition = footprint_definition(footprint, pins);
            let mut half = definition.body_size * 0.5;
            for pad in definition.pads {
                half = half.max(pad.offset.abs() + pad.size * 0.5);
            }
            (-half, half)
        };
        let rotation = glam::Mat2::from_angle(rotation.to_radians());
        let corners = [min, Vec2::new(max.x, min.y), max, Vec2::new(min.x, max.y)]
            .map(|p| rotation * p + position);
        (
            corners
                .iter()
                .copied()
                .fold(Vec2::splat(f32::INFINITY), Vec2::min),
            corners
                .iter()
                .copied()
                .fold(Vec2::splat(f32::NEG_INFINITY), Vec2::max),
        )
    }

    fn initial_position(
        &self,
        component: &crate::component::ElectronicComponent,
        placed: &[PcbComponentPlacement],
        existing: &HashMap<Uuid, PcbComponentPlacement>,
    ) -> Vec2 {
        let (local_min, local_max) = self.placement_bounds(
            &component.footprint,
            component.pins.len(),
            Vec2::ZERO,
            component.rotation,
        );
        let size = local_max - local_min;
        let (board_min, board_max) = self
            .board_outline
            .bounds()
            .unwrap_or((Vec2::ZERO, Vec2::new(420.0, 280.0)));
        let gap = 5.0;
        let occupied: Vec<_> = existing
            .values()
            .chain(placed.iter())
            .map(|p| self.placement_bounds(&p.footprint, p.pad_nets.len(), p.position, p.rotation))
            .collect();
        let mut corner = board_min + Vec2::splat(gap);
        loop {
            if let Some((_, max)) = occupied.iter().find(|(min, max)| {
                corner.x < max.x + gap
                    && corner.x + size.x + gap > min.x
                    && corner.y < max.y + gap
                    && corner.y + size.y + gap > min.y
            }) {
                corner.x = max.x + gap;
                if corner.x + size.x > board_max.x - gap {
                    corner.x = board_min.x + gap;
                    corner.y = occupied
                        .iter()
                        .filter(|(min, max)| {
                            corner.y < max.y + gap && corner.y + size.y + gap > min.y
                        })
                        .map(|(_, max)| max.y + gap)
                        .fold(corner.y + gap, f32::max);
                }
            } else {
                return corner - local_min;
            }
        }
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
            let pad_nets = self.component_pad_nets(index, component, &netlist);

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
                    position: self.initial_position(component, &synced, &existing),
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
        for via in &mut self.vias {
            if let Some(Some(name)) = renamed.get(&via.net) {
                via.net = name.clone();
            } else {
                via.orphaned = true;
            }
            // An orphaned via remains visible and is rejected by physical DRC.
            // Never silently erase copper when the schematic changes.
        }
        let old_rules = self.rules.nets.clone();
        for (old_name, entry) in old_rules {
            if let Some(Some(name)) = renamed.get(&old_name) {
                if name != &old_name {
                    self.rules
                        .nets
                        .entry(name.clone())
                        .and_modify(|rules| {
                            rules.clearance = rules.clearance.max(entry.clearance);
                            rules.min_width = rules.min_width.max(entry.min_width);
                        })
                        .or_insert(entry);
                }
            }
        }
        for trace in orphaned {
            if !self.removed_traces.iter().any(|old| old.id == trace.id) {
                self.removed_traces.push(trace);
            }
        }
        self.rebuild_airwires();
        summary
    }

    /// Routing uses the pad's physical copper, independently of the face used
    /// to draw its body. In particular a plated connector can start on either face.
    pub fn pad_supports_layer(
        &self,
        component_id: Uuid,
        pad_index: usize,
        layer: PcbLayer,
    ) -> bool {
        let Some(component) = self
            .components
            .iter()
            .find(|c| c.component_id == component_id)
        else {
            return false;
        };
        if let Some(asset) = self.footprint_assets.get(&component.footprint) {
            return asset.pads.get(pad_index).is_some_and(|pad| {
                super::geometry::copper_layers(pad, component.layer).contains(&layer)
            });
        }
        component.layer == layer && pad_index < self.resolved_footprint(component).pads.len()
    }

    pub fn pad_world_position(&self, component_index: usize, pad_index: usize) -> Option<Vec2> {
        let component = self.components.get(component_index)?;
        let footprint = self.resolved_footprint(component);
        let pad = footprint.pads.get(pad_index)?;
        let physical_asset = self.footprint_assets.contains_key(&component.footprint);
        let position = super::geometry::transform_footprint_point(
            component,
            super::footprint_asset::PointMm {
                x: pad.offset.x as f64,
                y: pad.offset.y as f64,
            },
            physical_asset,
        );
        position.is_finite().then_some(position)
    }

    /// Finds same-net traces that physically touch this component's copper
    /// pads, using their complete pad outlines and drill voids instead of pad
    /// centers. Call before replacing an assigned footprint to park routes
    /// attached to geometry that is about to move.
    pub fn traces_touching_component(&self, component_id: Uuid) -> Vec<Uuid> {
        let items = copper_items(self);
        let mut touching = BTreeSet::new();
        for pad in items.iter().filter(|item| {
            item.source == component_id && item.pad.is_some() && !item.net.is_empty()
        }) {
            for trace in items
                .iter()
                .filter(|item| item.pad.is_none() && item.layer == pad.layer && item.net == pad.net)
            {
                if trace.shape.distance(&pad.shape) <= CONTACT_EPSILON {
                    touching.insert(trace.source);
                }
            }
        }
        touching.into_iter().collect()
    }

    /// Resolves physical pad positions without consulting a mutable global library.
    /// Legacy artwork remains readable until explicitly assigned a physical asset.
    pub fn resolved_footprint(
        &self,
        component: &PcbComponentPlacement,
    ) -> super::FootprintDefinition {
        let Some(asset) = self.footprint_assets.get(&component.footprint) else {
            return footprint_definition(&component.footprint, component.pad_nets.len().max(1));
        };
        let size = asset.dimensions_mm().unwrap_or_default();
        super::FootprintDefinition {
            id: component.footprint.clone(),
            body_size: Vec2::new(size.width as f32, size.height as f32),
            pads: asset
                .pads
                .iter()
                .map(|pad| super::FootprintPadDefinition {
                    name: pad.number.clone(),
                    offset: Vec2::new(pad.position_mm.x as f32, pad.position_mm.y as f32),
                    size: Vec2::new(pad.size_mm.width as f32, pad.size_mm.height as f32),
                })
                .collect(),
            preview_asset: None,
        }
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
                && items[a].contact_shape().distance(items[b].contact_shape()) <= CONTACT_EPSILON
            {
                dsu.union(a, b);
            }
        });
        let mut plated_pad_layers = BTreeMap::<(Uuid, usize, usize), Vec<usize>>::new();
        for (index, item) in items.iter().enumerate() {
            if item.plated_through_hole {
                if let Some((component_index, pad_index)) = item.pad {
                    plated_pad_layers
                        .entry((item.source, component_index, pad_index))
                        .or_default()
                        .push(index);
                } else {
                    plated_pad_layers
                        .entry((item.source, usize::MAX, usize::MAX))
                        .or_default()
                        .push(index);
                }
            }
        }
        for layer_items in plated_pad_layers.values() {
            if let Some((&first, rest)) = layer_items.split_first() {
                for &index in rest {
                    dsu.union(first, index);
                }
            }
        }
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
    #[test]
    fn initial_placement_separates_large_battery_from_neighbors_and_preserves_edits() {
        use crate::component::ElectronicComponent;
        let mut schematic = crate::schematic::Schematic::new("Placement");
        for _ in 0..5 {
            schematic.add_component(ElectronicComponent::led());
        }
        schematic.add_component(ElectronicComponent::dc_source(9.0));
        for _ in 0..5 {
            schematic.add_component(ElectronicComponent::resistor("1k"));
        }
        let mut pcb = super::PcbLayout::new("Placement");
        pcb.sync_from_schematic(&schematic);
        for (i, a) in pcb.components.iter().enumerate() {
            let (min, max) =
                pcb.placement_bounds(&a.footprint, a.pad_nets.len(), a.position, a.rotation);
            for b in pcb.components.iter().skip(i + 1) {
                let (bmin, bmax) =
                    pcb.placement_bounds(&b.footprint, b.pad_nets.len(), b.position, b.rotation);
                assert!(max.x <= bmin.x || bmax.x <= min.x || max.y <= bmin.y || bmax.y <= min.y);
            }
        }
        pcb.components[0].position = glam::Vec2::new(33.0, 44.0);
        let placements = pcb.components.clone();
        pcb.sync_from_schematic(&schematic);
        assert_eq!(pcb.components, placements);
    }
    #[test]
    fn curve_clearance_bounds_do_not_create_false_electrical_contact() {
        use crate::pcb::{parse_kicad_footprint, PcbComponentPlacement, PcbLayer};
        let asset=parse_kicad_footprint(r#"(footprint "ContactProof" (pad "1" smd circle (at 0 0) (size 2 2) (layers "F.Cu" "F.Mask")))"#).unwrap();
        let mut layout = super::PcbLayout::new("proof");
        for x in [5.0, 7.0004] {
            layout.components.push(PcbComponentPlacement {
                component_id: uuid::Uuid::new_v4(),
                designator: "P".into(),
                value: String::new(),
                footprint: asset.id.clone(),
                position: glam::Vec2::new(x, 5.0),
                rotation: 0.0,
                layer: PcbLayer::TopCopper,
                locked: false,
                image_asset: None,
                pad_nets: vec!["LINK".into()],
            });
        }
        layout.footprint_assets.insert(asset.id.clone(), asset);
        layout.rebuild_airwires();
        assert_eq!(
            layout.airwires.len(),
            1,
            "A positive physical gap must remain unrouted"
        );
    }
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

    #[test]
    fn physical_asset_maps_duplicate_numbered_pads_by_schematic_pin_name() {
        let mut schematic = Schematic::new("Board");
        let component_id = schematic.add_component(ElectronicComponent::resistor("10k"));
        let asset = super::super::footprint_asset::parse_kicad_footprint(
            r#"(footprint "DuplicatePads"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "2" smd rect (at 0 0) (size 1 1) (layers "F.Cu"))
                (pad "1" smd rect (at 2 0) (size 1 1) (layers "F.Cu"))
                (pad "1" smd rect (at 4 0) (size 1 1) (layers "F.Cu"))
                (pad "" np_thru_hole circle (at 6 0) (size 1 1) (drill 0.4) (layers "*.Mask"))
            )"#,
        )
        .unwrap();
        schematic
            .components
            .iter_mut()
            .find(|component| component.id == component_id)
            .unwrap()
            .footprint = asset.id.clone();
        let netlist = schematic.netlist();

        let mut layout = PcbLayout::new("Board");
        layout.footprint_assets.insert(asset.id.clone(), asset);
        layout.sync_from_schematic(&schematic);

        let pad_nets = &layout.components[0].pad_nets;
        assert_eq!(pad_nets.len(), 4);
        assert_eq!(pad_nets[0], netlist.net_for_pin(0, 1).unwrap().name);
        assert_eq!(pad_nets[1], netlist.net_for_pin(0, 0).unwrap().name);
        assert!(
            layout.matches_schematic(&schematic),
            "Physical pad order is distinct from logical pin order"
        );
        assert_eq!(pad_nets[1], pad_nets[2]);
        assert!(pad_nets[3].is_empty(), "mechanical NPTH pads have no net");
    }

    #[test]
    fn physical_asset_uses_explicit_logical_pin_to_pad_mapping() {
        let mut schematic = Schematic::new("Board");
        let component_id = schematic.add_component(ElectronicComponent::led());
        let asset = super::super::footprint_asset::parse_kicad_footprint(
            r#"(footprint "LedPads"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "2" smd rect (at 0 0) (size 1 1) (layers "F.Cu"))
                (pad "1" smd rect (at 2 0) (size 1 1) (layers "F.Cu"))
            )"#,
        )
        .unwrap();
        let asset_id = asset.id.clone();
        let component = schematic
            .components
            .iter_mut()
            .find(|component| component.id == component_id)
            .unwrap();
        component.footprint = asset_id.clone();
        component.footprint_pin_map.insert("A".into(), "1".into());
        component.footprint_pin_map.insert("K".into(), "2".into());
        let netlist = schematic.netlist();

        let mut layout = PcbLayout::new("Board");
        layout.footprint_assets.insert(asset_id, asset);
        layout.sync_from_schematic(&schematic);

        let pad_nets = &layout.components[0].pad_nets;
        assert_eq!(pad_nets[0], netlist.net_for_pin(0, 1).unwrap().name);
        assert_eq!(pad_nets[1], netlist.net_for_pin(0, 0).unwrap().name);
        assert!(layout.matches_schematic(&schematic));
        schematic.components[0]
            .footprint_pin_map
            .insert("A".into(), "2".into());
        schematic.components[0]
            .footprint_pin_map
            .insert("K".into(), "1".into());
        assert!(
            !layout.matches_schematic(&schematic),
            "Changed mapping requires synchronization"
        );
        layout.sync_from_schematic(&schematic);
        assert!(layout.matches_schematic(&schematic));
        schematic.components[0].value = "different value".into();
        assert!(
            !layout.matches_schematic(&schematic),
            "Outdated component metadata is not synchronized"
        );
    }

    #[test]
    fn touching_trace_detection_uses_physical_pad_area_and_net() {
        let asset = super::super::footprint_asset::parse_kicad_footprint(
            r#"(footprint "TraceTouch"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "1" smd circle (at 0 0) (size 1 1) (layers "F.Cu"))
            )"#,
        )
        .unwrap();
        let asset_id = asset.id.clone();
        let component_id = Uuid::new_v4();
        let mut layout = PcbLayout::new("Board");
        layout.footprint_assets.insert(asset_id.clone(), asset);
        layout.components.push(PcbComponentPlacement {
            component_id,
            designator: "U1".into(),
            value: "test".into(),
            footprint: asset_id,
            position: Vec2::new(10.0, 10.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["N1".into()],
        });
        let touching_id = Uuid::new_v4();
        layout.traces.push(PcbTrace {
            id: touching_id,
            net: "N1".into(),
            layer: PcbLayer::TopCopper,
            width: 0.2,
            points: vec![Vec2::new(9.0, 10.0), Vec2::new(10.0, 10.0)],
        });
        layout.traces.push(PcbTrace {
            id: Uuid::new_v4(),
            net: "N2".into(),
            layer: PcbLayer::TopCopper,
            width: 0.2,
            points: vec![Vec2::new(9.0, 10.0), Vec2::new(10.0, 10.0)],
        });
        layout.traces.push(PcbTrace {
            id: Uuid::new_v4(),
            net: "N1".into(),
            layer: PcbLayer::BottomCopper,
            width: 0.2,
            points: vec![Vec2::new(9.0, 10.0), Vec2::new(10.0, 10.0)],
        });

        assert_eq!(
            layout.traces_touching_component(component_id),
            vec![touching_id]
        );
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
