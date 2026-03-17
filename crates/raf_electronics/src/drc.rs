//! Design Rule Check (DRC) / Electrical Rule Check (ERC).
//!
//! Validates a schematic against a set of electrical rules and
//! returns a structured report with errors, warnings, and info.

use crate::component::{ElectronicComponent, SimModel};
use crate::netlist::Netlist;
use crate::schematic::Schematic;
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

    DrcReport {
        errors,
        warnings,
        info,
    }
}

/// Rule 1: Find pins that are alone in their net (not connected to anything).
fn check_floating_pins(
    schematic: &Schematic,
    netlist: &Netlist,
    issues: &mut Vec<DrcIssue>,
) {
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
fn check_missing_values(
    schematic: &Schematic,
    issues: &mut Vec<DrcIssue>,
) {
    for comp in &schematic.components {
        let needs_value = matches!(
            comp.sim_model,
            SimModel::Resistor { .. } | SimModel::Capacitor { .. }
        );
        if needs_value && comp.value.trim().is_empty() {
            issues.push(DrcIssue {
                severity: DrcSeverity::Warning,
                rule: "missing_value".to_string(),
                message: format!(
                    "Component {} has no value assigned",
                    comp.designator
                ),
                components: vec![comp.id],
                location: Some(comp.position),
            });
        }
    }
}

/// Rule 3: Components with no pin connected to any other component.
fn check_isolated_components(
    schematic: &Schematic,
    netlist: &Netlist,
    issues: &mut Vec<DrcIssue>,
) {
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
fn check_unnamed_nets(
    schematic: &Schematic,
    issues: &mut Vec<DrcIssue>,
) {
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
fn check_short_circuit(
    schematic: &Schematic,
    netlist: &Netlist,
    issues: &mut Vec<DrcIssue>,
) {
    use crate::component::PinDirection;

    for net in &netlist.nets {
        let mut power_count = 0usize;
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

            if pin.direction == PinDirection::Power {
                power_count += 1;
                power_comps.push(comp.id);
            }
        }

        if power_count > 1 {
            issues.push(DrcIssue {
                severity: DrcSeverity::Error,
                rule: "short_circuit".to_string(),
                message: format!(
                    "Potential short circuit: {} power sources on net '{}'",
                    power_count, net.name
                ),
                components: power_comps,
                location: None,
            });
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
