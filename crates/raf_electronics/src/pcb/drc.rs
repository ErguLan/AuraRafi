use super::footprint_asset::FootprintPadTechnology;
use super::geometry::{
    copper_items, nearby_pairs, segment_distance, Copper, Shape, CONTACT_EPSILON,
};
use super::{PcbLayer, PcbLayout, PcbTrace};
use crate::drc::{DrcIssue, DrcReport, DrcSeverity};
use glam::Vec2;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

pub const DEFAULT_CLEARANCE_MM: f32 = 0.2;
/// Generic editing profile; a fabricator-specific profile may require more.
pub const MIN_TRACE_WIDTH_MM: f32 = 0.1;

struct DrillObstacle {
    source: Uuid,
    pad_index: usize,
    net: Option<String>,
    shape: Shape,
}

fn drill_obstacles(layout: &PcbLayout) -> Vec<DrillObstacle> {
    let mut holes = Vec::new();
    for placement in &layout.components {
        let Some(asset) = layout
            .footprint_assets
            .get(&placement.footprint)
            .filter(|a| a.is_fabrication_ready())
        else {
            continue;
        };
        for (pad_index, pad) in asset.pads.iter().enumerate() {
            let Some(outline) = pad.contact_drill_polygon_mm(64) else {
                continue;
            };
            holes.push(DrillObstacle {
                source: placement.component_id,
                pad_index,
                net: matches!(pad.technology, FootprintPadTechnology::PlatedThroughHole).then(
                    || {
                        placement
                            .pad_nets
                            .get(pad_index)
                            .cloned()
                            .unwrap_or_default()
                    },
                ),
                shape: Shape::Pad {
                    outline: outline
                        .into_iter()
                        .map(|p| super::geometry::transform_footprint_point(placement, p, true))
                        .collect(),
                    holes: Vec::new(),
                },
            });
        }
    }
    for via in &layout.vias {
        if via.position.is_finite() && via.drill.is_finite() && via.drill > 0.0 {
            holes.push(DrillObstacle {
                source: via.id,
                pad_index: usize::MAX,
                net: (!via.orphaned).then(|| via.net.clone()),
                shape: Shape::Pad {
                    outline: super::geometry::circle_polygon(
                        via.position,
                        via.drill * 0.5 / (std::f32::consts::PI / 64.0).cos(),
                    ),
                    holes: Vec::new(),
                },
            });
        }
    }
    holes
}

fn drill_interferes(hole: &DrillObstacle, item: &Copper, clearance: f32) -> bool {
    if item.source == hole.source && item.pad.is_some_and(|(_, pi)| pi == hole.pad_index) {
        return false;
    }
    if hole
        .net
        .as_ref()
        .is_some_and(|net| !net.is_empty() && net == &item.net)
    {
        return false;
    }
    hole.shape.distance(&item.shape) + CONTACT_EPSILON < clearance
}

impl PcbLayout {
    fn via_is_valid(&self, via: &super::PcbVia) -> bool {
        !via.orphaned
            && via.position.is_finite()
            && via.diameter.is_finite()
            && via.drill.is_finite()
            && via.drill >= self.rules.min_drill
            && via.diameter - via.drill + CONTACT_EPSILON >= 2.0 * self.rules.min_annular_ring
            && !via.net.trim().is_empty()
            && (self.net_pins.contains_key(&via.net)
                || self
                    .components
                    .iter()
                    .any(|c| c.pad_nets.contains(&via.net)))
    }

    /// Add an actual plated via. Both faces, drill obstacles and board rules are
    /// checked before committing; a failed attempt leaves the document intact.
    pub fn try_add_via(
        &mut self,
        net: &str,
        position: Vec2,
        diameter: f32,
        drill: f32,
    ) -> Result<Uuid, String> {
        let via = super::PcbVia {
            id: Uuid::new_v4(),
            net: net.into(),
            position,
            diameter,
            drill,
            orphaned: false,
        };
        if !self.rules.is_valid() || !self.via_is_valid(&via) {
            return Err("Invalid via: check net, drill and annular ring.".into());
        }
        if !super::geometry_outline_is_simple(&self.board_outline.points) {
            return Err("Fix the PCB outline before placing a via.".into());
        }
        let mut candidate = self.clone();
        candidate.vias.push(via.clone());
        let copper = copper_items(&candidate);
        let new_copper: Vec<_> = copper.iter().filter(|item| item.source == via.id).collect();
        if new_copper.iter().any(|item| {
            !item
                .shape
                .inside_outline(&self.board_outline.points, self.rules.edge_clearance)
        }) {
            return Err("Via violates the PCB edge clearance.".into());
        }
        if new_copper.iter().any(|a| {
            copper.iter().any(|b| {
                a.source != b.source
                    && a.layer == b.layer
                    && (b.net.is_empty() || a.net != b.net)
                    && a.shape.distance(&b.shape) + CONTACT_EPSILON
                        < self.rules.clearance(&a.net, &b.net)
            })
        }) {
            return Err("Via violates copper clearance.".into());
        }
        let holes = drill_obstacles(&candidate);
        let hole = holes
            .iter()
            .find(|h| h.source == via.id)
            .ok_or("Missing via drill")?;
        if holes.iter().any(|other| {
            other.source != via.id
                && hole.shape.distance(&other.shape) + CONTACT_EPSILON
                    < self.rules.default.clearance
        }) || copper
            .iter()
            .any(|item| drill_interferes(hole, item, self.rules.clearance(net, &item.net)))
            || holes.iter().filter(|h| h.source != via.id).any(|h| {
                new_copper.iter().any(|item| {
                    drill_interferes(
                        h,
                        item,
                        self.rules.clearance(h.net.as_deref().unwrap_or(""), net),
                    )
                })
            })
        {
            return Err("Via violates drill clearance.".into());
        }
        let id = via.id;
        self.vias.push(via);
        self.rebuild_airwires();
        Ok(id)
    }

    /// PCB checks operate on actual copper and layer geometry, independently
    /// of schematic ERC. Coordinates use the existing millimetre canvas scale.
    pub fn run_drc(&self) -> DrcReport {
        self.run_drc_cancellable(&AtomicBool::new(false))
            .expect("uncancelled PCB check")
    }

    pub fn run_drc_cancellable(&self, cancel: &AtomicBool) -> Option<DrcReport> {
        if cancel.load(Ordering::Acquire) {
            return None;
        }
        let mut report = DrcReport {
            errors: Vec::new(),
            warnings: Vec::new(),
            info: Vec::new(),
        };
        let mut error =
            |rule: &str, message: String, source: Option<Uuid>, location: Option<Vec2>| {
                report.errors.push(DrcIssue {
                    severity: DrcSeverity::Error,
                    rule: rule.into(),
                    message,
                    components: source.into_iter().collect(),
                    location,
                });
            };
        let outline = &self.board_outline.points;
        if !self.rules.is_valid() {
            error(
                "invalid_design_rules",
                "Design rules must be finite, positive and preserve the via annular ring.".into(),
                None,
                None,
            );
            return Some(report);
        }
        for via in &self.vias {
            if !self.via_is_valid(via) {
                error(
                    "invalid_via",
                    "Via needs a known net, finite geometry, minimum drill and annular ring."
                        .into(),
                    Some(via.id),
                    via.position.is_finite().then_some(via.position),
                );
            }
        }
        let valid_outline = outline_is_valid(outline, cancel)?;
        if !valid_outline {
            error(
                "invalid_board_outline",
                "PCB outline must be closed, finite and non-self-intersecting.".into(),
                None,
                None,
            );
        }
        for component in &self.components {
            if cancel.load(Ordering::Acquire) {
                return None;
            }
            if component.footprint.trim().is_empty() {
                error(
                    "missing_footprint",
                    format!("{} has no footprint.", component.designator),
                    Some(component.component_id),
                    Some(component.position),
                );
            }
            if !component.position.is_finite() || !component.rotation.is_finite() {
                error(
                    "invalid_pcb_geometry",
                    format!(
                        "{} has invalid placement coordinates.",
                        component.designator
                    ),
                    Some(component.component_id),
                    None,
                );
            }
            if let Some(asset) = self.footprint_assets.get(&component.footprint) {
                for pad in &asset.pads {
                    if let Some(drill) = &pad.drill {
                        let minimum = match drill.shape {
                            super::FootprintDrillShape::Circular { diameter_mm } => diameter_mm,
                            super::FootprintDrillShape::Oval {
                                width_mm,
                                height_mm,
                            } => width_mm.min(height_mm),
                        };
                        if minimum + (CONTACT_EPSILON as f64) < self.rules.min_drill as f64 {
                            error(
                                "minimum_drill",
                                format!(
                                    "{} pad {} violates minimum drill.",
                                    component.designator, pad.number
                                ),
                                Some(component.component_id),
                                Some(component.position),
                            );
                        }
                    }
                    if matches!(pad.technology, FootprintPadTechnology::PlatedThroughHole) {
                        if let (Some(land), Some(hole)) = (
                            pad.contact_outline_polygon_mm(64),
                            pad.contact_drill_polygon_mm(64),
                        ) {
                            let land: Vec<_> = land
                                .into_iter()
                                .map(|p| Vec2::new(p.x as f32, p.y as f32))
                                .collect();
                            let hole: Vec<_> = hole
                                .into_iter()
                                .map(|p| Vec2::new(p.x as f32, p.y as f32))
                                .collect();
                            let ring_ok = !land.is_empty()
                                && !hole.is_empty()
                                && hole
                                    .iter()
                                    .all(|p| super::geometry::point_in_polygon(*p, &land))
                                && (0..hole.len()).all(|i| {
                                    (0..land.len()).all(|j| {
                                        segment_distance(
                                            hole[i],
                                            hole[(i + 1) % hole.len()],
                                            land[j],
                                            land[(j + 1) % land.len()],
                                        ) + CONTACT_EPSILON
                                            >= self.rules.min_annular_ring
                                    })
                                });
                            if !ring_ok {
                                error(
                                    "annular_ring",
                                    format!(
                                        "{} pad {} violates minimum annular ring.",
                                        component.designator, pad.number
                                    ),
                                    Some(component.component_id),
                                    Some(component.position),
                                );
                            }
                        }
                    }
                }
            }
        }
        for trace in &self.traces {
            if cancel.load(Ordering::Acquire) {
                return None;
            }
            if trace.points.len() < 2
                || trace
                    .points
                    .windows(2)
                    .all(|pair| pair[0].distance(pair[1]) <= CONTACT_EPSILON)
                || trace.points.iter().any(|point| !point.is_finite())
                || !trace.width.is_finite()
                || trace.width < self.rules.for_net(&trace.net).min_width
            {
                error(
                    "invalid_trace",
                    format!(
                        "Trace '{}' needs finite coordinates and width >= {} mm.",
                        trace.net,
                        self.rules.for_net(&trace.net).min_width
                    ),
                    None,
                    trace.points.first().copied(),
                );
            }
        }
        let items = copper_items(self);
        if valid_outline {
            for item in &items {
                if cancel.load(Ordering::Acquire) {
                    return None;
                }
                if !item
                    .shape
                    .inside_outline(outline, self.rules.edge_clearance)
                {
                    let (a, b) = item.shape.bounds();
                    error(
                        "copper_outside_board",
                        format!(
                            "Copper on net '{}' violates the PCB edge clearance.",
                            item.net
                        ),
                        Some(item.source),
                        Some((a + b) * 0.5),
                    );
                }
            }
        }
        super::geometry::nearby_pairs_cancellable(
            &items,
            self.rules.max_clearance(),
            cancel,
            |a, b| {
                if items[a].net == items[b].net && !items[a].net.is_empty() {
                    return;
                }
                let clearance = self.rules.clearance(&items[a].net, &items[b].net);
                if items[a].shape.distance(&items[b].shape) + CONTACT_EPSILON < clearance {
                    let (lo, hi) = items[a].shape.bounds();
                    error(
                        "copper_clearance",
                        format!(
                            "Nets '{}' and '{}' violate {:.2} mm clearance on {}.",
                            items[a].net,
                            items[b].net,
                            clearance,
                            items[a].layer.display_name()
                        ),
                        Some(items[a].source),
                        Some((lo + hi) * 0.5),
                    );
                }
            },
        );
        let holes = drill_obstacles(self);
        for (index, hole) in holes.iter().enumerate() {
            if cancel.load(Ordering::Acquire) {
                return None;
            }
            let (lo, hi) = hole.shape.bounds();
            if holes.iter().skip(index + 1).any(|other| {
                hole.shape.distance(&other.shape) + CONTACT_EPSILON < self.rules.default.clearance
            }) {
                error(
                    "drill_drill_clearance",
                    "Drills overlap or violate the minimum spacing.".into(),
                    Some(hole.source),
                    Some((lo + hi) * 0.5),
                );
            }
            if valid_outline
                && !hole
                    .shape
                    .inside_outline(outline, self.rules.edge_clearance)
            {
                error(
                    "drill_outside_board",
                    "Drill violates the PCB edge clearance.".into(),
                    Some(hole.source),
                    Some((lo + hi) * 0.5),
                );
            }
            if items.iter().any(|item| {
                drill_interferes(
                    hole,
                    item,
                    self.rules
                        .clearance(hole.net.as_deref().unwrap_or(""), &item.net),
                )
            }) {
                error(
                    "drill_copper_clearance",
                    "Drill intersects unrelated copper or violates its clearance.".into(),
                    Some(hole.source),
                    Some((lo + hi) * 0.5),
                );
            }
        }
        if cancel.load(Ordering::Acquire) {
            return None;
        }
        if !self.removed_traces.is_empty() {
            report.warnings.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "orphaned_traces".into(),
                message: format!(
                    "{} traces await explicit net reassignment or discard.",
                    self.removed_traces.len()
                ),
                components: Vec::new(),
                location: None,
            });
        }
        for via in self.vias.iter().filter(|via| self.via_is_valid(via)) {
            let connected = items
                .iter()
                .filter(|item| item.source == via.id)
                .any(|land| {
                    items.iter().any(|other| {
                        other.source != via.id
                            && other.layer == land.layer
                            && other.net == via.net
                            && land.contact_shape().distance(other.contact_shape())
                                <= CONTACT_EPSILON
                    })
                });
            if !connected {
                report.warnings.push(DrcIssue {
                    severity: DrcSeverity::Warning,
                    rule: "unconnected_via".into(),
                    message: format!("Via on '{}' has no physical copper connection.", via.net),
                    components: vec![via.id],
                    location: Some(via.position),
                });
            }
        }
        // Recompute connectivity instead of trusting serialized/stale airwires.
        let mut connected = self.clone();
        if !connected.rebuild_airwires_cancellable(cancel) {
            return None;
        }
        for wire in connected.airwires {
            report.warnings.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "unrouted_net".into(),
                message: format!("Net '{}' has unconnected copper islands.", wire.net),
                components: vec![wire.from_component_id, wire.to_component_id],
                location: Some(wire.from),
            });
        }
        Some(report)
    }

    /// Runs editor DRC plus strict manufacturing-readiness checks. The legacy
    /// `run_drc()` remains useful for boards that still use illustrative
    /// footprints; this preflight rejects those until a physical asset and a
    /// complete per-pad net snapshot are available.
    pub fn run_manufacturing_preflight(&self) -> DrcReport {
        let mut report = self.run_drc();

        for component in &self.components {
            let Some(asset) = self.footprint_assets.get(&component.footprint) else {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Error,
                    "unresolved_physical_footprint",
                    format!(
                        "{} has no resolved physical footprint asset for '{}'.",
                        component.designator, component.footprint
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
                continue;
            };
            if asset.id != component.footprint {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Error,
                    "unresolved_physical_footprint",
                    format!(
                        "{} resolves '{}' to an asset with mismatched id '{}'.",
                        component.designator, component.footprint, asset.id
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }

            let validation = asset.validate();
            for issue in &validation {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Error,
                    "invalid_physical_footprint",
                    format!(
                        "{} footprint {}: {}.",
                        component.designator, issue.location, issue.message
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }

            let mut has_fabrication_blocker = !validation.is_empty();
            for feature in &asset.unsupported_features {
                let severity = if feature.blocks_fabrication {
                    has_fabrication_blocker = true;
                    DrcSeverity::Error
                } else {
                    DrcSeverity::Info
                };
                manufacturing_issue(
                    &mut report,
                    severity,
                    if feature.blocks_fabrication {
                        "unsupported_physical_footprint_feature"
                    } else {
                        "unsupported_footprint_metadata"
                    },
                    format!(
                        "{} footprint has {} at {}.",
                        component.designator, feature.feature, feature.location
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }
            if !asset.is_fabrication_ready() && !has_fabrication_blocker {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Error,
                    "unsupported_physical_footprint_feature",
                    format!(
                        "{} footprint contains physical data that the manufacturing model cannot represent.",
                        component.designator
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }

            if component.pad_nets.len() != asset.pads.len() {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Error,
                    "footprint_pad_net_mapping",
                    format!(
                        "{} has {} pad-net entries for {} physical pads.",
                        component.designator,
                        component.pad_nets.len(),
                        asset.pads.len()
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }
            for (index, (pad, net)) in asset.pads.iter().zip(&component.pad_nets).enumerate() {
                if matches!(
                    &pad.technology,
                    FootprintPadTechnology::NonPlatedThroughHole
                ) && !net.trim().is_empty()
                {
                    manufacturing_issue(
                        &mut report,
                        DrcSeverity::Error,
                        "footprint_pad_net_mapping",
                        format!(
                            "{} mechanical NPTH pad {} at index {} must not carry net '{}'.",
                            component.designator, pad.number, index, net
                        ),
                        Some(component.component_id),
                        Some(component.position),
                    );
                } else if !matches!(pad.technology, FootprintPadTechnology::NonPlatedThroughHole)
                    && net.trim().is_empty()
                {
                    manufacturing_issue(
                        &mut report,
                        DrcSeverity::Error,
                        "footprint_pad_net_mapping",
                        format!(
                            "{} pad {} has no logical pin/net assignment.",
                            component.designator, pad.number
                        ),
                        Some(component.component_id),
                        Some(component.position),
                    );
                } else if !net.trim().is_empty() && !self.net_pins.contains_key(net) {
                    manufacturing_issue(
                        &mut report,
                        DrcSeverity::Error,
                        "footprint_pad_net_mapping",
                        format!(
                            "{} pad {} references unknown PCB net '{}'.",
                            component.designator, pad.number, net
                        ),
                        Some(component.component_id),
                        Some(component.position),
                    );
                }
            }

            if asset
                .provenance
                .license
                .as_deref()
                .is_none_or(str::is_empty)
            {
                manufacturing_issue(
                    &mut report,
                    DrcSeverity::Info,
                    "footprint_license_unrecorded",
                    format!(
                        "{} footprint '{}' has no recorded license in its provenance.",
                        component.designator, asset.name
                    ),
                    Some(component.component_id),
                    Some(component.position),
                );
            }
        }

        // These are warnings in the editing DRC so users can keep working on
        // a partially routed board. A manufacturing preflight must treat them
        // as blockers until the missing routes are resolved or reviewed.
        for issue in report.warnings.iter().filter(|issue| {
            matches!(
                issue.rule.as_str(),
                "unrouted_net" | "orphaned_traces" | "unconnected_via"
            )
        }) {
            report.errors.push(DrcIssue {
                severity: DrcSeverity::Error,
                rule: format!("manufacturing_{}", issue.rule),
                message: issue.message.clone(),
                components: issue.components.clone(),
                location: issue.location,
            });
        }
        report
    }

    pub fn try_route_airwire(&mut self, index: usize) -> Result<Uuid, String> {
        let wire = self
            .airwires
            .get(index)
            .cloned()
            .ok_or("Airwire not found.")?;
        let items = copper_items(self);
        let from_layers = endpoint_copper_layers(
            self,
            &items,
            wire.from_component_id,
            wire.from,
            &wire.net,
            "Source",
        )?;
        let to_layers = endpoint_copper_layers(
            self,
            &items,
            wire.to_component_id,
            wire.to,
            &wire.net,
            "Destination",
        )?;
        let layer = [PcbLayer::TopCopper, PcbLayer::BottomCopper]
            .into_iter()
            .find(|layer| from_layers.contains(layer) && to_layers.contains(layer))
            .ok_or("Airwire endpoints have no common copper layer; a physical layer transition is required.")?;
        self.try_route_airwire_with_options(index, layer, raf_core::units::DEFAULT_TRACE_WIDTH_MM)
    }

    /// Routes one airwire on the selected copper layer and trace width.
    /// Endpoint layers come from the resolved pad copper, including PTH pads
    /// that conduct on both faces, rather than the component's placement side.
    pub fn try_route_airwire_with_options(
        &mut self,
        index: usize,
        layer: PcbLayer,
        width: f32,
    ) -> Result<Uuid, String> {
        let wire = self
            .airwires
            .get(index)
            .cloned()
            .ok_or("Airwire not found.")?;
        let items = copper_items(self);
        let from_layers = endpoint_copper_layers(
            self,
            &items,
            wire.from_component_id,
            wire.from,
            &wire.net,
            "Source",
        )?;
        let to_layers = endpoint_copper_layers(
            self,
            &items,
            wire.to_component_id,
            wire.to,
            &wire.net,
            "Destination",
        )?;
        if !from_layers.contains(&layer) || !to_layers.contains(&layer) {
            return Err(format!(
                "Both airwire pads must have copper on {}.",
                layer.display_name()
            ));
        }
        if !outline_is_valid(&self.board_outline.points, &AtomicBool::new(false)).unwrap_or(false) {
            return Err("Fix the PCB outline before routing.".into());
        }
        // Try both orthogonal elbows. Neither attempt is committed until it
        // passes copper and outline checks; existing unrelated errors remain.
        for corner in [
            Vec2::new(wire.to.x, wire.from.y),
            Vec2::new(wire.from.x, wire.to.y),
        ] {
            let mut points = vec![wire.from];
            if corner.distance(wire.from) > CONTACT_EPSILON
                && corner.distance(wire.to) > CONTACT_EPSILON
            {
                points.push(corner);
            }
            points.push(wire.to);
            match self.try_add_trace(&wire.net, layer, width, points) {
                Ok(id) => return Ok(id),
                Err(_) => {}
            }
        }
        Err("Route blocked by copper clearance or the PCB outline. Move the obstructing geometry and try again.".into())
    }

    /// Adds a physical trace only after validating its geometry, board-edge
    /// clearance, and copper clearance. Failed attempts leave the layout intact.
    pub fn try_add_trace(
        &mut self,
        net: &str,
        layer: PcbLayer,
        width: f32,
        points: Vec<Vec2>,
    ) -> Result<Uuid, String> {
        if net.trim().is_empty()
            || (!self.net_pins.contains_key(net)
                && !self
                    .components
                    .iter()
                    .any(|component| component.pad_nets.iter().any(|pad_net| pad_net == net)))
        {
            return Err(format!("Unknown or empty PCB net '{net}'."));
        }
        if !self.rules.is_valid() {
            return Err("Invalid PCB design rules.".into());
        }
        if !width.is_finite() || width < self.rules.for_net(net).min_width {
            return Err(format!(
                "Trace width must be finite and at least {} mm.",
                self.rules.for_net(net).min_width
            ));
        }
        if points.len() < 2
            || points.iter().any(|point| !point.is_finite())
            || points
                .windows(2)
                .all(|pair| pair[0].distance(pair[1]) <= CONTACT_EPSILON)
        {
            return Err("A trace needs at least two finite points.".into());
        }
        if !outline_is_valid(&self.board_outline.points, &AtomicBool::new(false)).unwrap_or(false) {
            return Err("Fix the PCB outline before routing.".into());
        }
        let id = Uuid::new_v4();
        let mut items = copper_items(self);
        let candidate = points
            .windows(2)
            .map(|pair| Copper {
                contact_shape: None,
                source: id,
                net: net.to_string(),
                layer,
                shape: Shape::Segment(pair[0], pair[1], width * 0.5),
                pad: None,
                plated_through_hole: false,
            })
            .collect::<Vec<_>>();
        if candidate.is_empty()
            || candidate.iter().any(|item| {
                !item
                    .shape
                    .inside_outline(&self.board_outline.points, self.rules.edge_clearance)
            })
        {
            return Err("Trace violates the PCB edge clearance.".into());
        }
        if drill_obstacles(self).iter().any(|hole| {
            candidate.iter().any(|item| {
                drill_interferes(
                    hole,
                    item,
                    self.rules
                        .clearance(hole.net.as_deref().unwrap_or(""), &item.net),
                )
            })
        }) {
            return Err("Trace violates drill clearance.".into());
        }
        items.extend(candidate);
        let mut blocked = false;
        nearby_pairs(&items, self.rules.max_clearance(), |a, b| {
            if (items[a].source == id || items[b].source == id)
                && (items[a].net.is_empty()
                    || items[b].net.is_empty()
                    || items[a].net != items[b].net)
                && items[a].shape.distance(&items[b].shape) + CONTACT_EPSILON
                    < self.rules.clearance(&items[a].net, &items[b].net)
            {
                blocked = true;
            }
        });
        if blocked {
            return Err("Trace violates copper clearance.".into());
        }
        self.traces.push(PcbTrace {
            id,
            net: net.to_string(),
            layer,
            width,
            points,
        });
        self.rebuild_airwires();
        Ok(id)
    }

    /// Restore parked work only after assigning an existing electrical net.
    pub fn restore_removed_trace(&mut self, id: Uuid, net: &str) -> bool {
        if !self
            .components
            .iter()
            .any(|c| c.pad_nets.iter().any(|name| name == net))
        {
            return false;
        }
        let Some(index) = self.removed_traces.iter().position(|trace| trace.id == id) else {
            return false;
        };
        let mut trace = self.removed_traces.remove(index);
        trace.net = net.to_string();
        self.traces.push(trace);
        self.rebuild_airwires();
        true
    }
}

fn endpoint_copper_layers(
    layout: &PcbLayout,
    items: &[Copper],
    component_id: Uuid,
    point: Vec2,
    net: &str,
    label: &str,
) -> Result<Vec<PcbLayer>, String> {
    let Some((component_index, component)) = layout
        .components
        .iter()
        .enumerate()
        .find(|(_, component)| component.component_id == component_id)
    else {
        return Err(format!(
            "{label} component moved or is missing; rebuild PCB connectivity before routing."
        ));
    };
    let pad_indices = component
        .pad_nets
        .iter()
        .enumerate()
        .filter(|(_, pad_net)| pad_net.as_str() == net)
        .filter_map(|(pad_index, _)| {
            layout
                .pad_world_position(component_index, pad_index)
                .filter(|position| position.distance(point) <= CONTACT_EPSILON)
                .map(|_| pad_index)
        })
        .collect::<Vec<_>>();
    if pad_indices.is_empty() {
        return Err(format!(
            "{label} pad moved or is missing; rebuild PCB connectivity before routing."
        ));
    }
    let mut layers = Vec::new();
    for item in items.iter().filter(|item| item.source == component_id) {
        if item.net == net
            && item
                .pad
                .is_some_and(|(ci, pi)| ci == component_index && pad_indices.contains(&pi))
            && !layers.contains(&item.layer)
        {
            layers.push(item.layer);
        }
    }
    if layers.is_empty() {
        return Err(format!("{label} pad has no copper on a routable layer."));
    }
    Ok(layers)
}

fn manufacturing_issue(
    report: &mut DrcReport,
    severity: DrcSeverity,
    rule: &str,
    message: String,
    component: Option<Uuid>,
    location: Option<Vec2>,
) {
    let issue = DrcIssue {
        severity,
        rule: rule.to_string(),
        message,
        components: component.into_iter().collect(),
        location,
    };
    match severity {
        DrcSeverity::Error => report.errors.push(issue),
        DrcSeverity::Warning => report.warnings.push(issue),
        DrcSeverity::Info => report.info.push(issue),
    }
}

/// Shared by the report and route preflight, without running connectivity or
/// unrelated copper checks merely to validate the outline.
pub(super) fn outline_is_valid(outline: &[Vec2], cancel: &AtomicBool) -> Option<bool> {
    let closed = outline.len() >= 4
        && outline
            .first()
            .zip(outline.last())
            .is_some_and(|(a, b)| a.distance(*b) <= CONTACT_EPSILON)
        && outline.iter().all(|p| p.is_finite());
    let area: f32 = outline
        .windows(2)
        .map(|edge| edge[0].perp_dot(edge[1]))
        .sum();
    if !closed || !area.is_finite() || area.abs() <= CONTACT_EPSILON {
        return Some(false);
    }
    for a in 0..outline.len() - 1 {
        for b in a + 2..outline.len() - 1 {
            if cancel.load(Ordering::Acquire) {
                return None;
            }
            if a == 0 && b == outline.len() - 2 {
                continue;
            }
            if segment_distance(outline[a], outline[a + 1], outline[b], outline[b + 1])
                <= CONTACT_EPSILON
            {
                return Some(false);
            }
        }
    }
    Some(true)
}

#[cfg(test)]
mod tests {
    #[test]
    fn npth_drill_is_an_obstacle_on_both_copper_faces() {
        let asset=super::super::parse_kicad_footprint(r#"(footprint "MechanicalDrill" (pad "" np_thru_hole circle (at 0 0) (size 2 2) (drill 2) (layers "*.Cu" "*.Mask")))"#).unwrap();
        assert!(asset.is_fabrication_ready());
        let mut layout = PcbLayout::new("holes");
        let source = Uuid::new_v4();
        layout.components.push(super::super::PcbComponentPlacement {
            component_id: source,
            designator: "H".into(),
            value: String::new(),
            footprint: asset.id.clone(),
            position: Vec2::splat(20.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec![String::new()],
        });
        layout.footprint_assets.insert(asset.id.clone(), asset);
        layout.net_pins.insert("LINK".into(), vec![Uuid::new_v4()]);
        for layer in [PcbLayer::TopCopper, PcbLayer::BottomCopper] {
            assert!(layout
                .try_add_trace(
                    "LINK",
                    layer,
                    0.25,
                    vec![Vec2::new(10.0, 20.0), Vec2::new(30.0, 20.0)]
                )
                .is_err());
            assert!(layout.traces.is_empty());
        }
        layout.traces.push(PcbTrace {
            id: Uuid::new_v4(),
            net: "LINK".into(),
            layer: PcbLayer::BottomCopper,
            width: 0.25,
            points: vec![Vec2::new(10.0, 20.0), Vec2::new(30.0, 20.0)],
        });
        assert!(layout
            .run_drc()
            .errors
            .iter()
            .any(|issue| issue.rule == "drill_copper_clearance"));
    }
    use super::*;
    use crate::pcb::footprint_asset::parse_kicad_footprint;
    use crate::pcb::{PcbComponentPlacement, PcbLayer};

    fn component(
        component_id: Uuid,
        designator: &str,
        footprint: &str,
        position: Vec2,
        layer: PcbLayer,
        net: &str,
    ) -> PcbComponentPlacement {
        PcbComponentPlacement {
            component_id,
            designator: designator.into(),
            value: "test".into(),
            footprint: footprint.into(),
            position,
            rotation: 0.0,
            layer,
            locked: false,
            image_asset: None,
            pad_nets: vec![net.into()],
        }
    }

    fn plated_and_bottom_smd_layout() -> PcbLayout {
        let pth = parse_kicad_footprint(
            r#"(footprint "PthOne"
                (version 20240108) (generator pcbnew) (layer "F.Cu")
                (pad "1" thru_hole circle (at 0 0) (size 2 2) (drill 0.8) (layers "*.Cu" "*.Mask"))
            )"#,
        )
        .unwrap();
        let smd = parse_kicad_footprint(
            r#"(footprint "BottomSmd"
                (version 20240108) (generator pcbnew) (layer "F.Cu")
                (pad "1" smd rect (at 0 0) (size 2 2) (layers "F.Cu" "F.Mask" "F.Paste"))
            )"#,
        )
        .unwrap();
        assert!(pth.is_fabrication_ready(), "{:?}", pth.validate());
        assert!(smd.is_fabrication_ready(), "{:?}", smd.validate());
        let pth_id = pth.id.clone();
        let smd_id = smd.id.clone();
        let mut layout = PcbLayout::new("Board");
        layout.footprint_assets.insert(pth_id.clone(), pth);
        layout.footprint_assets.insert(smd_id.clone(), smd);
        layout.components.push(component(
            Uuid::new_v4(),
            "J1",
            &pth_id,
            Vec2::new(20.0, 20.0),
            PcbLayer::TopCopper,
            "N1",
        ));
        layout.components.push(component(
            Uuid::new_v4(),
            "R1",
            &smd_id,
            Vec2::new(70.0, 20.0),
            PcbLayer::BottomCopper,
            "N1",
        ));
        layout.rebuild_airwires();
        assert_eq!(layout.airwires.len(), 1);
        layout
    }

    #[test]
    fn manufacturing_preflight_rejects_legacy_footprints_without_changing_editor_drc() {
        let mut layout = PcbLayout::new("Board");
        let component_id = Uuid::new_v4();
        layout.components.push(PcbComponentPlacement {
            component_id,
            designator: "U1".into(),
            value: "legacy".into(),
            footprint: "Legacy-Illustration".into(),
            position: Vec2::new(20.0, 20.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: vec!["N1".into()],
        });

        let editor_report = layout.run_drc();
        assert!(editor_report
            .errors
            .iter()
            .all(|issue| issue.rule != "unresolved_physical_footprint"));

        let preflight = layout.run_manufacturing_preflight();
        assert!(preflight.errors.iter().any(|issue| {
            issue.rule == "unresolved_physical_footprint" && issue.components == vec![component_id]
        }));
    }

    #[test]
    fn manufacturing_preflight_reports_unsupported_geometry_and_pad_mapping() {
        let asset = super::super::footprint_asset::parse_kicad_footprint(
            r#"(footprint "UnsupportedPad"
                (version 20240108)
                (generator pcbnew)
                (layer "F.Cu")
                (pad "1" smd custom (at 0 0) (size 1 1) (layers "F.Cu"))
            )"#,
        )
        .unwrap();
        let asset_id = asset.id.clone();
        let mut layout = PcbLayout::new("Board");
        layout.footprint_assets.insert(asset_id.clone(), asset);
        layout.components.push(PcbComponentPlacement {
            component_id: Uuid::new_v4(),
            designator: "U2".into(),
            value: "unsupported".into(),
            footprint: asset_id,
            position: Vec2::new(20.0, 20.0),
            rotation: 0.0,
            layer: PcbLayer::TopCopper,
            locked: false,
            image_asset: None,
            pad_nets: Vec::new(),
        });

        let preflight = layout.run_manufacturing_preflight();
        assert!(preflight
            .errors
            .iter()
            .any(|issue| { issue.rule == "unsupported_physical_footprint_feature" }));
        assert!(preflight
            .errors
            .iter()
            .any(|issue| issue.rule == "footprint_pad_net_mapping"));
    }

    #[test]
    fn airwire_routing_uses_pad_copper_layers_and_pth_connects_both_faces() {
        let mut layout = plated_and_bottom_smd_layout();
        let route_id = layout
            .try_route_airwire_with_options(0, PcbLayer::BottomCopper, 0.3)
            .expect("PTH should route to a bottom-side SMD on bottom copper");
        let route = layout
            .traces
            .iter()
            .find(|trace| trace.id == route_id)
            .unwrap();
        assert_eq!(route.layer, PcbLayer::BottomCopper);
        assert!((route.width - 0.3).abs() < f32::EPSILON);
        assert!(layout.airwires.is_empty());
    }

    #[test]
    fn automatic_airwire_route_chooses_common_pad_layer_and_wrong_layer_is_rejected() {
        let mut layout = plated_and_bottom_smd_layout();
        let error = layout
            .try_route_airwire_with_options(0, PcbLayer::TopCopper, 0.3)
            .expect_err("bottom-side SMD has no top copper");
        assert!(error.contains("Both airwire pads"));
        assert!(
            layout.traces.is_empty(),
            "failed attempt must not mutate routes"
        );

        let route_id = layout
            .try_route_airwire(0)
            .expect("automatic routing should select the common bottom layer");
        assert_eq!(
            layout
                .traces
                .iter()
                .find(|trace| trace.id == route_id)
                .unwrap()
                .layer,
            PcbLayer::BottomCopper
        );
    }

    #[test]
    fn manual_trace_rejects_clearance_failure_without_mutating_layout() {
        let mut layout = PcbLayout::new("Board");
        layout.components.push(component(
            Uuid::new_v4(),
            "R1",
            "legacy",
            Vec2::new(20.0, 20.0),
            PcbLayer::TopCopper,
            "N1",
        ));
        layout.traces.push(PcbTrace {
            id: Uuid::new_v4(),
            net: "N2".into(),
            layer: PcbLayer::TopCopper,
            width: 0.5,
            points: vec![Vec2::new(50.0, 40.0), Vec2::new(50.0, 60.0)],
        });
        let before_traces = layout.traces.clone();
        let before_airwires = layout.airwires.clone();

        let error = layout
            .try_add_trace(
                "N1",
                PcbLayer::TopCopper,
                0.5,
                vec![Vec2::new(40.0, 50.0), Vec2::new(60.0, 50.0)],
            )
            .expect_err("crossing nets violate copper clearance");

        assert!(error.contains("copper clearance"));
        assert_eq!(layout.traces, before_traces);
        assert_eq!(layout.airwires, before_airwires);
    }
}
