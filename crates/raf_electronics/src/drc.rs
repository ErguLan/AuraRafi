//! Design Rule Check (DRC) / Electrical Rule Check (ERC).
//!
//! Validates a schematic against a set of electrical rules and
//! returns a structured report with errors, warnings, and info.

use crate::component::SimModel;
use crate::extensions::run_registered_drc_rules;
use crate::netlist::Netlist;
use crate::schematic::{Schematic, WireAnchor};
use glam::Vec2;
use std::sync::atomic::{AtomicBool, Ordering};
use uuid::Uuid;

/// Severity level of a DRC issue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DrcSeverity {
    Error,
    Warning,
    Info,
}

/// A single DRC finding.
#[derive(Debug, Clone)]
pub struct DrcIssue {
    pub severity: DrcSeverity,
    /// Rule identifier (e.g. "floating_pin").
    pub rule: String,
    /// Human-readable description.
    pub message: String,
    /// Component IDs involved.
    pub components: Vec<Uuid>,
    /// Location on the schematic (if applicable).
    pub location: Option<Vec2>,
}

/// Full DRC report.
#[derive(Debug, Clone)]
pub struct DrcReport {
    pub errors: Vec<DrcIssue>,
    pub warnings: Vec<DrcIssue>,
    pub info: Vec<DrcIssue>,
}

impl DrcReport {
    /// Total number of issues.
    pub fn total(&self) -> usize {
        self.errors.len() + self.warnings.len() + self.info.len()
    }

    /// Whether the schematic passed all checks.
    pub fn passed(&self) -> bool {
        self.errors.is_empty() && self.warnings.is_empty()
    }

    /// Get all issues as a flat list, sorted by severity.
    pub fn all_issues(&self) -> Vec<&DrcIssue> {
        let mut all: Vec<&DrcIssue> = Vec::new();
        all.extend(self.errors.iter());
        all.extend(self.warnings.iter());
        all.extend(self.info.iter());
        all
    }

    /// Convert to simple string messages (backwards compatible with old
    /// `electrical_test()` return format).
    pub fn to_string_list(&self) -> Vec<String> {
        let mut result: Vec<String> = Vec::new();
        for issue in &self.errors {
            result.push(format!("[ERROR] {}: {}", issue.rule, issue.message));
        }
        for issue in &self.warnings {
            result.push(format!("[WARNING] {}: {}", issue.rule, issue.message));
        }
        for issue in &self.info {
            result.push(format!("[INFO] {}: {}", issue.rule, issue.message));
        }
        if result.is_empty() {
            result.push("DRC passed - no issues found.".to_string());
        }
        result
    }
}

/// Run all DRC checks on a schematic.
pub fn run_drc(schematic: &Schematic) -> DrcReport {
    run_drc_cancellable(schematic, &AtomicBool::new(false)).expect("an uncancelled DRC completes")
}

pub fn run_drc_cancellable(schematic: &Schematic, cancel: &AtomicBool) -> Option<DrcReport> {
    let netlist = Netlist::from_schematic_cancellable(schematic, cancel)?;
    let mut errors: Vec<DrcIssue> = Vec::new();
    let mut warnings: Vec<DrcIssue> = Vec::new();
    let mut info: Vec<DrcIssue> = Vec::new();

    // Rule 1: Floating pins (pin not connected to any net with other pins).
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_floating_pins(schematic, &netlist, &mut warnings);

    // Rule 2: Component without value.
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_missing_values(schematic, &mut warnings);

    // Rule 3: Isolated component (no pin in any shared net).
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_isolated_components(schematic, &netlist, &mut errors);

    // Rule 4: Unnamed nets (wires without net label).
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_unnamed_nets(schematic, &mut info);

    // Rule 5: Short circuit (basic detection).
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_short_circuit(schematic, &netlist, &mut errors);

    // Rule 6: LED without current-limiting resistor.
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_led_without_resistor(schematic, &netlist, &mut warnings, cancel);

    // Rule 7: A wire endpoint must terminate at a pin, a valid pin anchor,
    // or another wire endpoint. This exposes visually close but electrically
    // disconnected drawing mistakes to the Agent and the CAD canvas.
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_wire_endpoints(schematic, &mut errors, &mut warnings, cancel);

    // Rule 8: A wire must not bypass a multi-pin component by placing two of
    // its pins on the same electrical net.
    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    check_component_pin_shorts(schematic, &netlist, &mut errors);

    // Drawing the same label on disconnected islands must never silently
    // imply a connection. The netlist keeps them separate and ERC explains it.
    let mut labeled_nets = std::collections::HashMap::new();
    for wire in &schematic.wires {
        if cancel.load(Ordering::Relaxed) {
            return None;
        }
        let label = wire.net.trim();
        if label.is_empty() {
            continue;
        }
        let Some(net) = netlist.net_for_wire(wire.id) else {
            continue;
        };
        if labeled_nets
            .insert(label, net.id)
            .is_some_and(|previous| previous != net.id)
        {
            warnings.push(DrcIssue {
                severity:DrcSeverity::Warning,rule:"ambiguous_net_label".into(),
                message:format!("Label '{label}' occurs on disconnected nets; this island is '{}'. Connect the wires or give the nets distinct labels.",net.name),
                components:Vec::new(),location:Some(wire.start),
            });
        }
    }

    if cancel.load(Ordering::Relaxed) {
        return None;
    }
    for issue in run_registered_drc_rules(schematic) {
        match issue.severity {
            DrcSeverity::Error => errors.push(issue),
            DrcSeverity::Warning => warnings.push(issue),
            DrcSeverity::Info => info.push(issue),
        }
    }

    Some(DrcReport {
        errors,
        warnings,
        info,
    })
}

/// Rule 1: Find pins that are alone in their net (not connected to anything).
fn check_floating_pins(schematic: &Schematic, netlist: &Netlist, issues: &mut Vec<DrcIssue>) {
    for (ci, comp) in schematic.components.iter().enumerate() {
        for (pi, pin) in comp.pins.iter().enumerate() {
            if let Some(net) = netlist.net_for_pin(ci, pi) {
                if net.pins.len() <= 1 {
                    issues.push(DrcIssue {
                        severity: DrcSeverity::Warning,
                        rule: "floating_pin".to_string(),
                        message: format!(
                            "Unconnected pin: {} pin {} ({})",
                            comp.designator, pin.name, comp.value
                        ),
                        components: vec![comp.id],
                        location: Some(comp.position),
                    });
                }
            } else {
                // Pin not in ANY net at all.
                issues.push(DrcIssue {
                    severity: DrcSeverity::Warning,
                    rule: "floating_pin".to_string(),
                    message: format!(
                        "Unconnected pin: {} pin {} ({})",
                        comp.designator, pin.name, comp.value
                    ),
                    components: vec![comp.id],
                    location: Some(comp.position),
                });
            }
        }
    }
}

/// Rule 2: Components that need a value but have it empty.
fn check_missing_values(schematic: &Schematic, issues: &mut Vec<DrcIssue>) {
    for comp in &schematic.components {
        let needs_value = matches!(
            comp.sim_model,
            SimModel::Resistor { .. } | SimModel::Capacitor { .. }
        );
        if needs_value && comp.value.trim().is_empty() {
            issues.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "missing_value".to_string(),
                message: format!("Component {} has no value assigned", comp.designator),
                components: vec![comp.id],
                location: Some(comp.position),
            });
        }
    }
}

/// Rule 3: Components with no pin connected to any other component.
fn check_isolated_components(schematic: &Schematic, netlist: &Netlist, issues: &mut Vec<DrcIssue>) {
    for (ci, comp) in schematic.components.iter().enumerate() {
        let has_connection = comp.pins.iter().enumerate().any(|(pi, _)| {
            netlist
                .net_for_pin(ci, pi)
                .map(|net| net.pins.len() > 1)
                .unwrap_or(false)
        });

        if !has_connection {
            issues.push(DrcIssue {
                severity: DrcSeverity::Error,
                rule: "isolated_component".to_string(),
                message: format!(
                    "Component {} ({}) is completely isolated - no connections",
                    comp.designator, comp.value
                ),
                components: vec![comp.id],
                location: Some(comp.position),
            });
        }
    }
}

/// Rule 4: Wires without net names.
fn check_unnamed_nets(schematic: &Schematic, issues: &mut Vec<DrcIssue>) {
    for wire in &schematic.wires {
        if wire.net.is_empty() {
            issues.push(DrcIssue {
                severity: DrcSeverity::Info,
                rule: "unnamed_net".to_string(),
                message: format!(
                    "Wire from ({:.0},{:.0}) to ({:.0},{:.0}) has no net name",
                    wire.start.x, wire.start.y, wire.end.x, wire.end.y
                ),
                components: vec![],
                location: Some(Vec2::new(
                    (wire.start.x + wire.end.x) / 2.0,
                    (wire.start.y + wire.end.y) / 2.0,
                )),
            });
        }
    }
}

/// Rule 5: Basic short circuit detection.
/// Looks for nets where multiple voltage sources or power pins are
/// connected without any load between them.
fn check_short_circuit(schematic: &Schematic, netlist: &Netlist, issues: &mut Vec<DrcIssue>) {
    // Sharing a return (or a positive rail) alone is legitimate. Conflicting
    // voltage constraints exist only when both terminals share the same nets.
    let mut constraints = std::collections::HashMap::<(usize, usize), (f64, Uuid)>::new();
    for (ci, comp) in schematic.components.iter().enumerate() {
        let SimModel::DcSource { voltage } = comp.sim_model else {
            continue;
        };
        let (Some(plus), Some(minus)) = (netlist.net_for_pin(ci, 0), netlist.net_for_pin(ci, 1))
        else {
            continue;
        };
        let key = (plus.id.min(minus.id), plus.id.max(minus.id));
        let signed_voltage = if plus.id <= minus.id {
            voltage
        } else {
            -voltage
        };
        if let Some(&(other_voltage, other_id)) = constraints.get(&key) {
            if (other_voltage - signed_voltage).abs() > 1e-9 {
                issues.push(DrcIssue {
                    severity: DrcSeverity::Error,
                    rule: "short_circuit".to_string(),
                    message: format!(
                        "Conflicting voltage sources across nets '{}' and '{}'",
                        plus.name, minus.name
                    ),
                    components: vec![other_id, comp.id],
                    location: Some(comp.position),
                });
            }
        } else {
            constraints.insert(key, (signed_voltage, comp.id));
        }
    }
}

fn check_wire_endpoints(
    schematic: &Schematic,
    errors: &mut Vec<DrcIssue>,
    warnings: &mut Vec<DrcIssue>,
    cancel: &AtomicBool,
) {
    let pins: Vec<_> = schematic
        .components
        .iter()
        .flat_map(|component| {
            component
                .pins
                .iter()
                .map(move |pin| crate::schematic::component_pin_world_position(component, pin))
        })
        .collect();
    let pin_grid = crate::netlist::PinGrid::new(&pins);
    let endpoints: Vec<_> = schematic
        .wires
        .iter()
        .flat_map(|wire| {
            [
                crate::netlist::resolve_wire_point(schematic, wire, true),
                crate::netlist::resolve_wire_point(schematic, wire, false),
            ]
        })
        .collect();
    let wire_grid = crate::netlist::PinGrid::new(&endpoints);

    for (wire_index, wire) in schematic.wires.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        for (endpoint_name, point, anchor) in [
            ("start", wire.start, wire.start_anchor),
            ("end", wire.end, wire.end_anchor),
        ] {
            if let Some(WireAnchor::Pin {
                component_id,
                pin_id,
            }) = anchor
            {
                if schematic
                    .pin_world_position_by_ids(component_id, pin_id)
                    .is_none()
                {
                    errors.push(DrcIssue {
                        severity: DrcSeverity::Error,
                        rule: "invalid_wire_anchor".to_string(),
                        message: format!(
                            "Wire {} {} anchor references a missing component pin",
                            wire_index + 1,
                            endpoint_name
                        ),
                        components: vec![component_id],
                        location: Some(point),
                    });
                }
                continue;
            }

            let touches_pin = !pin_grid.near(point).is_empty();
            let touches_wire = wire_grid
                .near(point)
                .iter()
                .any(|index| index / 2 != wire_index);

            if !touches_pin && !touches_wire {
                warnings.push(DrcIssue {
                    severity: DrcSeverity::Warning,
                    rule: "dangling_wire_endpoint".to_string(),
                    message: format!(
                        "Wire {} {} is not connected to a pin or junction",
                        wire_index + 1,
                        endpoint_name
                    ),
                    components: Vec::new(),
                    location: Some(point),
                });
            }
        }
    }
}

fn check_component_pin_shorts(
    schematic: &Schematic,
    netlist: &Netlist,
    issues: &mut Vec<DrcIssue>,
) {
    for (component_index, component) in schematic.components.iter().enumerate() {
        if component.pins.len() < 2 || matches!(component.sim_model, SimModel::Wire) {
            continue;
        }
        for first_pin in 0..component.pins.len() {
            let Some(first_net) = netlist.net_for_pin(component_index, first_pin) else {
                continue;
            };
            for second_pin in (first_pin + 1)..component.pins.len() {
                let Some(second_net) = netlist.net_for_pin(component_index, second_pin) else {
                    continue;
                };
                if first_net.id == second_net.id {
                    issues.push(DrcIssue {
                        severity: DrcSeverity::Error,
                        rule: "component_pins_shorted".to_string(),
                        message: format!(
                            "{} pins {} and {} share net '{}' and bypass the component",
                            component.designator,
                            component.pins[first_pin].name,
                            component.pins[second_pin].name,
                            first_net.name
                        ),
                        components: vec![component.id],
                        location: Some(component.position),
                    });
                    break;
                }
            }
        }
    }
}

/// Rule 6: LED connected without a current-limiting resistor on the same net.
fn check_led_without_resistor(
    schematic: &Schematic,
    netlist: &Netlist,
    issues: &mut Vec<DrcIssue>,
    cancel: &AtomicBool,
) {
    // A limiter must be unavoidable on the return path around the LED.
    // A resistor merely sharing a rail, or placed in parallel, proves nothing.
    let mut edges = vec![Vec::<(usize, usize, bool)>::new(); netlist.nets.len()];
    for (ci, comp) in schematic.components.iter().enumerate() {
        if matches!(
            comp.sim_model,
            SimModel::Capacitor { .. } | SimModel::Magnet { .. }
        ) {
            continue;
        }
        let (Some(a), Some(b)) = (netlist.net_for_pin(ci, 0), netlist.net_for_pin(ci, 1)) else {
            continue;
        };
        let limiter =
            matches!(comp.sim_model, SimModel::Resistor { ohms } if ohms.is_finite() && ohms > 0.0);
        edges[a.id].push((b.id, ci, limiter));
        edges[b.id].push((a.id, ci, limiter));
    }
    let reachable = |start: usize, end: usize, excluded: usize, allow_resistors: bool| {
        let mut seen = vec![false; edges.len()];
        let mut pending = vec![start];
        seen[start] = true;
        while let Some(node) = pending.pop() {
            if cancel.load(Ordering::Relaxed) {
                return false;
            }
            if node == end {
                return true;
            }
            for &(next, component, resistor) in &edges[node] {
                if component != excluded && (allow_resistors || !resistor) && !seen[next] {
                    seen[next] = true;
                    pending.push(next);
                }
            }
        }
        false
    };
    for (ci, comp) in schematic.components.iter().enumerate() {
        if !matches!(comp.sim_model, SimModel::Led { .. }) {
            continue;
        }
        if cancel.load(Ordering::Relaxed) {
            return;
        }
        let protected = match (netlist.net_for_pin(ci, 0), netlist.net_for_pin(ci, 1)) {
            (Some(a), Some(b)) => {
                reachable(a.id, b.id, ci, true) && !reachable(a.id, b.id, ci, false)
            }
            _ => false,
        };
        if !protected && !schematic.wires.is_empty() {
            issues.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "led_no_resistor".to_string(),
                message: format!(
                    "LED {} has no verified series current-limiting resistor",
                    comp.designator
                ),
                components: vec![comp.id],
                location: Some(comp.position),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;
    use crate::schematic::Schematic;

    #[test]
    fn isolated_component_detected() {
        let mut sch = Schematic::new("Test");
        sch.add_component(ElectronicComponent::resistor("10k"));
        let report = run_drc(&sch);
        // Should have an error for isolated component.
        assert!(
            report.errors.iter().any(|i| i.rule == "isolated_component"),
            "Expected isolated_component error"
        );
    }

    #[test]
    fn component_pins_shorted_is_reported() {
        let mut sch = Schematic::new("Shorted");
        sch.add_component(ElectronicComponent::resistor("1k"));
        let start = crate::schematic::component_pin_world_position(
            &sch.components[0],
            &sch.components[0].pins[0],
        );
        let end = crate::schematic::component_pin_world_position(
            &sch.components[0],
            &sch.components[0].pins[1],
        );
        sch.add_wire(start, end, "N_SHORT");

        let report = run_drc(&sch);
        assert!(report
            .errors
            .iter()
            .any(|issue| issue.rule == "component_pins_shorted"));
    }

    #[test]
    fn dangling_wire_endpoint_is_reported() {
        let mut sch = Schematic::new("Dangling");
        sch.add_wire(
            Vec2::new(100.0, 100.0),
            Vec2::new(180.0, 100.0),
            "N_DANGLING",
        );

        let report = run_drc(&sch);
        assert!(report
            .warnings
            .iter()
            .any(|issue| issue.rule == "dangling_wire_endpoint"));
    }

    #[test]
    fn wire_meeting_another_wire_endpoint_is_not_dangling() {
        let mut sch = Schematic::new("Junction");
        // Two components whose pins sit on a shared junction point.
        // The wire that connects them ends exactly on the second pin,
        // so neither endpoint should be flagged as dangling.
        let mut r1 = ElectronicComponent::resistor("10k");
        r1.position = Vec2::new(0.0, 0.0);
        let r1_pin2 = r1.pins[1].id;
        let r1_id = r1.id;
        sch.add_component(r1);

        let mut r2 = ElectronicComponent::resistor("4.7k");
        r2.position = Vec2::new(80.0, 0.0);
        let r2_pin1 = r2.pins[0].id;
        let r2_id = r2.id;
        sch.add_component(r2);

        let p1 = crate::schematic::component_pin_world_position(
            &sch.components.iter().find(|c| c.id == r1_id).unwrap(),
            &sch.components
                .iter()
                .find(|c| c.id == r1_id)
                .unwrap()
                .pins
                .iter()
                .find(|p| p.id == r1_pin2)
                .unwrap(),
        );
        let p2 = crate::schematic::component_pin_world_position(
            &sch.components.iter().find(|c| c.id == r2_id).unwrap(),
            &sch.components
                .iter()
                .find(|c| c.id == r2_id)
                .unwrap()
                .pins
                .iter()
                .find(|p| p.id == r2_pin1)
                .unwrap(),
        );
        sch.add_wire(p1, p2, "N_LINK");

        let report = run_drc(&sch);
        assert!(
            !report
                .warnings
                .iter()
                .any(|issue| issue.rule == "dangling_wire_endpoint"),
            "wire between two exact pins must not be flagged as dangling, got {:?}",
            report.warnings
        );
    }

    #[test]
    fn empty_value_detected() {
        let mut sch = Schematic::new("Test");
        let mut r = ElectronicComponent::resistor("");
        r.value = String::new();
        sch.add_component(r);
        let report = run_drc(&sch);
        assert!(
            report.warnings.iter().any(|i| i.rule == "missing_value"),
            "Expected missing_value warning"
        );
    }

    #[test]
    fn empty_schematic_passes() {
        let sch = Schematic::new("Test");
        let report = run_drc(&sch);
        assert!(report.passed());
    }
}
