//! Report lines, simulation readout and inspector value commits.

use super::*;

/// Structured classification for one DRC or simulation line.
///
/// The dock used to recover severity by searching the rendered text for English
/// words such as "issues" or "converged", which silently dropped any
/// `[ERROR] short_circuit:` line into the neutral color and broke the moment the
/// text was translated. The kind travels with the line instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsReportLineKind {
    /// Neutral state line such as "running", "cancelled" or "obsolete".
    Status,
    Running,
    Passed,
    Issues,
    Failed,
    /// Free text produced by the analysis backend.
    Message,
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElectronicsReportLine {
    pub kind: ElectronicsReportLineKind,
    pub text: String,
    /// Component the finding is about, when the rule named one.
    ///
    /// Carrying the identity here (instead of letting the surface track a row
    /// index) keeps `error -> object` navigation stable while the report grows
    /// a new leading status line.
    pub target: Option<Uuid>,
}

impl ElectronicsReportLine {
    fn new(kind: ElectronicsReportLineKind, text: impl Into<String>) -> Self {
        Self {
            kind,
            text: text.into(),
            target: None,
        }
    }

    /// Row that resolves to a component in the document.
    fn targeted(
        kind: ElectronicsReportLineKind,
        text: impl Into<String>,
        target: Option<Uuid>,
    ) -> Self {
        Self {
            kind,
            text: text.into(),
            target,
        }
    }
}

impl NativeElectronicsEditor {
    pub fn drc_lines(&self) -> &[ElectronicsReportLine] {
        &self.drc_lines
    }

    pub fn drc_report(&self) -> Option<&DrcReport> {
        self.drc_report.as_ref()
    }

    pub fn simulation_lines(&self) -> &[ElectronicsReportLine] {
        &self.simulation_lines
    }

    /// True when the document changed after the visible DRC result was produced.
    /// The report stays available so a surface can dim the markers and explain
    /// that they describe an older revision.
    pub fn drc_is_stale(&self) -> bool {
        self.drc_stale
            || self
                .drc_report_surface
                .is_some_and(|surface| surface != self.active_surface)
    }

    /// True when the simulation result no longer matches the document.
    pub fn simulation_is_stale(&self) -> bool {
        self.simulation_stale
    }

    /// True when the schematic and the PCB no longer describe the same design.
    pub fn sync_is_stale(&self) -> bool {
        self.sync_stale
    }

    pub fn simulation_results(&self) -> Option<&SimulationResults> {
        self.simulation_results.as_ref()
    }

    pub(super) fn push_report_line(
        &mut self,
        kind: AnalysisKind,
        line_kind: ElectronicsReportLineKind,
        key: &str,
    ) {
        let line = ElectronicsReportLine::new(line_kind, i18n::t(key, self.language));
        match kind {
            AnalysisKind::Drc => self.drc_lines.push(line),
            AnalysisKind::Simulation => self.simulation_lines.push(line),
        }
    }

    pub(super) fn accept_simulation_result(
        &mut self,
        result: SimulationResults,
        netlist: &raf_electronics::Netlist,
    ) {
        if self.lab.dc_requested {
            self.lab.accept_dc(
                &self.schematic,
                self.built_schematic_fingerprint,
                netlist,
                &result,
            );
        }
        let mut lines = vec![ElectronicsReportLine::new(
            if result.converged {
                ElectronicsReportLineKind::Passed
            } else {
                ElectronicsReportLineKind::Issues
            },
            i18n::t(
                if result.converged {
                    "electronics.analysis.simulation_converged"
                } else {
                    "electronics.analysis.simulation_not_converged"
                },
                self.language,
            ),
        )];
        lines.push(metric_line(
            "electronics.analysis.simulation_nodes",
            result.node_voltages.len(),
            self.language,
        ));
        lines.push(metric_line(
            "electronics.analysis.simulation_components",
            result.component_currents.len(),
            self.language,
        ));
        lines.extend(result.messages.iter().map(|message| {
            ElectronicsReportLine::new(ElectronicsReportLineKind::Message, message.clone())
        }));
        if result.converged {
            lines.push(ElectronicsReportLine::new(
                ElectronicsReportLineKind::Info,
                format!(
                    "{}: {:.3e} A · {:.3e} W",
                    i18n::t("electronics.analysis.dc_balance", self.language),
                    result.max_kcl_residual_amps,
                    result.power_balance_watts
                ),
            ));
        }
        for net in &netlist.nets {
            if let Some(voltage) = result.node_voltages.get(&net.id) {
                let target = net
                    .pins
                    .first()
                    .map(|&(ci, _)| self.schematic.components[ci].id);
                lines.push(ElectronicsReportLine::targeted(
                    ElectronicsReportLineKind::Info,
                    format!("{}: {:.6} V", net.name, voltage),
                    target,
                ));
            }
        }
        for (ci, component) in self.schematic.components.iter().enumerate() {
            if let (Some(current), Some(power)) = (
                result.component_currents.get(&ci),
                result.component_power.get(&ci),
            ) {
                let voltage = netlist
                    .net_for_pin(ci, 0)
                    .and_then(|n| result.node_voltages.get(&n.id))
                    .zip(
                        netlist
                            .net_for_pin(ci, 1)
                            .and_then(|n| result.node_voltages.get(&n.id)),
                    )
                    .map(|(a, b)| a - b)
                    .unwrap_or(0.0);
                lines.push(ElectronicsReportLine::targeted(
                    ElectronicsReportLineKind::Info,
                    format!(
                        "{}: {:.6} V | {:.6} A | {:.6} W",
                        component.designator, voltage, current, power
                    ),
                    Some(component.id),
                ));
            }
        }
        self.simulation_lines = lines;
        self.simulation_results = Some(result);
        self.simulation_stale = false;
        self.clear_surface_errors_for_key("electronics.error.simulation_failed");
        self.touch_ui();
    }

    pub(super) fn set_selected_value(&mut self, value: &str) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return false;
        }
        self.set_component_value(selection.source_id, value)
    }

    /// A deferred inspector commit belongs to the edited component, even if
    /// the same pointer press selected another object before it was applied.
    pub(super) fn set_component_value(&mut self, source_id: Uuid, value: &str) -> bool {
        if self.active_surface == CadSurfaceKind::Pcb {
            let before = self.snapshot();
            let Some(component) = self
                .pcb
                .components
                .iter_mut()
                .find(|component| component.component_id == source_id)
            else {
                return false;
            };
            if component.locked || component.value == value {
                return false;
            }
            component.value = value.to_string();
            if let Some(schematic_component) = self
                .schematic
                .components
                .iter_mut()
                .find(|component| component.id == source_id)
            {
                schematic_component.value = value.to_string();
                schematic_component.sync_sim_model_from_value();
            }
            self.history.record(before, &self.snapshot());
            self.dirty = true;
            self.rebuild_scene();
            self.touch_ui();
            return true;
        }
        let before = self.snapshot();
        let Some(component) = self
            .schematic
            .components
            .iter_mut()
            .find(|component| component.id == source_id)
        else {
            return false;
        };
        if component.locked || component.value == value {
            return false;
        }
        component.value = value.to_string();
        component.sync_sim_model_from_value();
        self.history.record(before, &self.snapshot());
        self.dirty = true;
        self.rebuild_scene();
        self.touch_ui();
        true
    }

}

/// Builds the structured DRC lines. Severity travels with the line, so no
/// surface has to guess the tone from the rendered text.
pub(super) fn report_lines(report: &DrcReport, language: Language) -> Vec<ElectronicsReportLine> {
    let mut lines = Vec::new();
    for issues in [&report.errors, &report.warnings, &report.info] {
        for issue in issues {
            let (kind, key) = match issue.severity {
                DrcSeverity::Error => (
                    ElectronicsReportLineKind::Error,
                    "electronics.analysis.severity_error",
                ),
                DrcSeverity::Warning => (
                    ElectronicsReportLineKind::Warning,
                    "electronics.analysis.severity_warning",
                ),
                DrcSeverity::Info => (
                    ElectronicsReportLineKind::Info,
                    "electronics.analysis.severity_info",
                ),
            };
            lines.push(ElectronicsReportLine::targeted(
                kind,
                format!(
                    "[{}] {}: {}",
                    i18n::t(key, language),
                    issue.rule,
                    issue.message
                ),
                issue.components.first().copied(),
            ));
        }
    }
    if lines.is_empty() {
        lines.push(ElectronicsReportLine::new(
            ElectronicsReportLineKind::Passed,
            i18n::t("electronics.analysis.drc_no_issues", language),
        ));
    }
    lines
}

fn metric_line(key: &str, value: usize, language: Language) -> ElectronicsReportLine {
    ElectronicsReportLine::new(
        ElectronicsReportLineKind::Status,
        format!("{}: {value}", i18n::t(key, language)),
    )
}
