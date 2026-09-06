//! Design Rule Check (DRC) / Electrical Rule Check (ERC).
//!
//! Validates a schematic against a set of electrical rules and
//! returns a structured report with errors, warnings, and info.

use crate::component::SimModel;
use crate::extensions::run_registered_drc_rules;
use crate::netlist::Netlist;
use crate::schematic::{Schematic, WireAnchor};
use glam::Vec2;
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
    let netlist = Netlist::from_schematic(schematic);
    let mut errors: Vec<DrcIssue> = Vec::new();
    let mut warnings: Vec<DrcIssue> = Vec::new();
    let mut info: Vec<DrcIssue> = Vec::new();

    // Rule 1: Floating pins (pin not connected to any net with other pins).
    check_floating_pins(schematic, &netlist, &mut warnings);

    // Rule 2: Component without value.
    check_missing_values(schematic, &mut warnings);

    // Rule 3: Isolated component (no pin in any shared net).
    check_isolated_components(schematic, &netlist, &mut errors);

    // Rule 4: Unnamed nets (wires without net label).
    check_unnamed_nets(schematic, &mut info);

    // Rule 5: Short circuit (basic detection).
    check_short_circuit(schematic, &netlist, &mut errors);

    // Rule 6: LED without current-limiting resistor.
    check_led_without_resistor(schematic, &netlist, &mut warnings);

    // Rule 7: A wire endpoint must terminate at a pin, a valid pin anchor,
    // or another wire endpoint. This exposes visually close but electrically
    // disconnected drawing mistakes to the Agent and the CAD canvas.
    check_wire_endpoints(schematic, &mut errors, &mut warnings);

    // Rule 8: A wire must not bypass a multi-pin component by placing two of
    // its pins on the same electrical net.
    check_component_pin_shorts(schematic, &netlist, &mut errors);

    for issue in run_registered_drc_rules(schematic) {
        match issue.severity {
            DrcSeverity::Error => errors.push(issue),
            DrcSeverity::Warning => warnings.push(issue),
            DrcSeverity::Info => info.push(issue),
        }
    }

    DrcReport {
        errors,
        warnings,
        info,
    }
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
    use crate::component::PinDirection;
    use std::collections::HashSet;

    for net in &netlist.nets {
        let mut power_components = HashSet::new();
        let mut power_comps: Vec<Uuid> = Vec::new();

        for &(ci, pi) in &net.pins {
            if ci >= schematic.components.len() {
                continue;
            }
            let comp = &schematic.components[ci];
            if pi >= comp.pins.len() {
                continue;
            }
            let pin = &comp.pins[pi];

            let is_power_source = pin.direction == PinDirection::Power
                || matches!(comp.sim_model, SimModel::DcSource { .. });
            if is_power_source && power_components.insert(comp.id) {
                power_comps.push(comp.id);
            }
        }

        if power_comps.len() > 1 {
            issues.push(DrcIssue {
                severity: DrcSeverity::Error,
                rule: "short_circuit".to_string(),
                message: format!(
                    "Potential short circuit: {} power sources on net '{}'",
                    power_comps.len(),
                    net.name
                ),
                components: power_comps,
                location: None,
            });
        }
    }
}

fn check_wire_endpoints(
    schematic: &Schematic,
    errors: &mut Vec<DrcIssue>,
    warnings: &mut Vec<DrcIssue>,
) {
    // Five percent of the 20-unit schematic grid. Anything beyond that and
    // the visual is unmistakably detached from a pin or junction.
    const ENDPOINT_TOLERANCE: f32 = 1.0;

    for (wire_index, wire) in schematic.wires.iter().enumerate() {
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

            let touches_pin = schematic.components.iter().any(|component| {
                component.pins.iter().any(|pin| {
                    crate::schematic::component_pin_world_position(component, pin).distance(point)
                        < ENDPOINT_TOLERANCE
                })
            });
            let touches_wire = schematic
                .wires
                .iter()
                .enumerate()
                .any(|(other_index, other)| {
                    other_index != wire_index
                        && (other.start.distance(point) < ENDPOINT_TOLERANCE
                            || other.end.distance(point) < ENDPOINT_TOLERANCE)
                });

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
) {
    for (ci, comp) in schematic.components.iter().enumerate() {
        if !matches!(comp.sim_model, SimModel::Led { .. }) {
            continue;
        }

        // Check each pin of the LED for a resistor on the same net.
        let mut has_resistor = false;
        for (pi, _pin) in comp.pins.iter().enumerate() {
            if let Some(net) = netlist.net_for_pin(ci, pi) {
                for &(other_ci, _other_pi) in &net.pins {
                    if other_ci == ci {
                        continue;
                    }
                    if other_ci < schematic.components.len() {
                        if matches!(
                            schematic.components[other_ci].sim_model,
                            SimModel::Resistor { .. }
                        ) {
                            has_resistor = true;
                            break;
                        }
                    }
                }
            }
            if has_resistor {
                break;
            }
        }

        if !has_resistor && !schematic.wires.is_empty() {
            issues.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "led_no_resistor".to_string(),
                message: format!(
                    "LED {} has no current-limiting resistor in its circuit",
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
