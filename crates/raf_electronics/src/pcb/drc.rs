use super::geometry::{copper_items, nearby_pairs, segment_distance, CONTACT_EPSILON};
use super::{PcbLayout, PcbTrace};
use crate::drc::{DrcIssue, DrcReport, DrcSeverity};
use glam::Vec2;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

pub const DEFAULT_CLEARANCE_MM: f32 = 0.2;

impl PcbLayout {
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
        }
        for trace in &self.traces {
            if cancel.load(Ordering::Acquire) {
                return None;
            }
            if trace.points.len() < 2
                || trace.points.iter().any(|point| !point.is_finite())
                || !trace.width.is_finite()
                || trace.width < raf_core::units::DEFAULT_TRACE_WIDTH_MM
            {
                error(
                    "invalid_trace",
                    format!(
                        "Trace '{}' needs finite coordinates and width >= {} mm.",
                        trace.net,
                        raf_core::units::DEFAULT_TRACE_WIDTH_MM
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
                if !item.shape.inside_outline(outline, DEFAULT_CLEARANCE_MM) {
                    let (a, b) = item.shape.bounds();
                    error(
                        "copper_outside_board",
                        format!(
                            "Copper on net '{}' violates the PCB edge clearance.",
                            item.net
                        ),
                        item.pad.map(|_| item.source),
                        Some((a + b) * 0.5),
                    );
                }
            }
        }
        super::geometry::nearby_pairs_cancellable(&items, DEFAULT_CLEARANCE_MM, cancel, |a, b| {
            if items[a].net == items[b].net && !items[a].net.is_empty() {
                return;
            }
            if items[a].shape.distance(&items[b].shape) + CONTACT_EPSILON < DEFAULT_CLEARANCE_MM {
                let (lo, hi) = items[a].shape.bounds();
                error(
                    "copper_clearance",
                    format!(
                        "Nets '{}' and '{}' violate {:.2} mm clearance on {}.",
                        items[a].net,
                        items[b].net,
                        DEFAULT_CLEARANCE_MM,
                        items[a].layer.display_name()
                    ),
                    items[a].pad.map(|_| items[a].source),
                    Some((lo + hi) * 0.5),
                );
            }
        });
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

    pub fn try_route_airwire(&mut self, index: usize) -> Result<Uuid, String> {
        let wire = self
            .airwires
            .get(index)
            .cloned()
            .ok_or("Airwire not found.")?;
        let endpoint_layer = |component_id: Uuid, point: Vec2| {
            self.components
                .iter()
                .enumerate()
                .find(|(_, c)| c.component_id == component_id)
                .and_then(|(ci, component)| {
                    component
                        .pad_nets
                        .iter()
                        .enumerate()
                        .find(|(pi, net)| {
                            **net == wire.net
                                && self.pad_world_position(ci, *pi).is_some_and(|position| {
                                    position.distance(point) <= CONTACT_EPSILON
                                })
                        })
                        .map(|_| component.layer)
                })
        };
        let from_layer = endpoint_layer(wire.from_component_id, wire.from)
            .ok_or("Source pad moved or is missing; rebuild PCB connectivity before routing.")?;
        let to_layer = endpoint_layer(wire.to_component_id, wire.to).ok_or(
            "Destination pad moved or is missing; rebuild PCB connectivity before routing.",
        )?;
        if from_layer != to_layer {
            return Err("Routing between layers requires a physical layer transition.".into());
        }
        if !outline_is_valid(&self.board_outline.points, &AtomicBool::new(false)).unwrap_or(false) {
            return Err("Fix the PCB outline before routing.".into());
        }
        let id = Uuid::new_v4();
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
            self.traces.push(PcbTrace {
                id,
                net: wire.net.clone(),
                layer: from_layer,
                width: raf_core::units::DEFAULT_TRACE_WIDTH_MM,
                points,
            });
            let items = copper_items(self);
            let mut blocked = items.iter().filter(|item| item.source == id).any(|item| {
                !item
                    .shape
                    .inside_outline(&self.board_outline.points, DEFAULT_CLEARANCE_MM)
            });
            nearby_pairs(&items, DEFAULT_CLEARANCE_MM, |a, b| {
                if (items[a].source == id || items[b].source == id)
                    && items[a].net != items[b].net
                    && items[a].shape.distance(&items[b].shape) + CONTACT_EPSILON
                        < DEFAULT_CLEARANCE_MM
                {
                    blocked = true;
                }
            });
            if !blocked {
                self.rebuild_airwires();
                return Ok(id);
            }
            self.traces.pop();
        }
        Err("Route blocked by copper clearance or the PCB outline. Move the obstructing geometry and try again.".into())
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

/// Shared by the report and route preflight, without running connectivity or
/// unrelated copper checks merely to validate the outline.
fn outline_is_valid(outline: &[Vec2], cancel: &AtomicBool) -> Option<bool> {
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
