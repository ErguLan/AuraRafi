//! Electronics-specific workbench model data.
//!
//! The shell owns layout and intent dispatch. This module owns only the
//! read-only projections used by the navigator and inspector, keeping
//! Electronics document/catalog logic out of the large workbench compositor.

use std::collections::HashSet;

use crate::electronics_controller::{
    ElectronicsReportLineKind, ElectronicsSelectionKind, NativeElectronicsEditor,
};
use crate::panels::electronics_navigator_surface::{
    ElectronicsLibraryEntry, ElectronicsNavigatorEntry,
};
use crate::panels::electronics_surface::{ElectronicsAnalysisLine, ElectronicsAnalysisTone};
use raf_core::{i18n, Language};
use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::UiIconId;

/// Retained dock tab ids of the two Electronics analysis panels.
pub(crate) const ELECTRONICS_ANALYSIS_DRC_TAB: &str = "drc";
pub(crate) const ELECTRONICS_ANALYSIS_SIMULATION_TAB: &str = "simulation";

/// Which analysis dock panel is being presented.
///
/// The dock used to pass the display string as the discriminant and the surface
/// decided DRC vs simulation with `title.contains("simulation")`, so any wording
/// change silently rendered the simulation tab as a DRC panel. The panel identity
/// is now an explicit enum owned here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ElectronicsAnalysisPanel {
    Drc,
    Simulation,
}

impl ElectronicsAnalysisPanel {
    /// Resolves a retained dock tab id. Unknown tabs are not an analysis panel.
    pub(crate) fn from_tab(tab: &str) -> Option<Self> {
        match tab {
            ELECTRONICS_ANALYSIS_DRC_TAB => Some(Self::Drc),
            ELECTRONICS_ANALYSIS_SIMULATION_TAB => Some(Self::Simulation),
            _ => None,
        }
    }

    pub(crate) fn tab_id(self) -> &'static str {
        match self {
            Self::Drc => ELECTRONICS_ANALYSIS_DRC_TAB,
            Self::Simulation => ELECTRONICS_ANALYSIS_SIMULATION_TAB,
        }
    }

    pub(crate) fn title_key(self) -> &'static str {
        match self {
            Self::Drc => "electronics.analysis.drc_title",
            Self::Simulation => "electronics.analysis.simulation_title",
        }
    }

    /// i18n title. Presentation only: the panel identity never depends on it.
    pub(crate) fn title(self, language: Language) -> String {
        i18n::t(self.title_key(), language)
    }
}

pub(crate) struct ElectronicsInspectorData {
    pub title: String,
    pub fields: Vec<(String, String)>,
    pub pins: Vec<(String, String)>,
}

pub(crate) fn inspector_data(editor: &NativeElectronicsEditor) -> Option<ElectronicsInspectorData> {
    let selection = editor.selection()?;

    if editor.active_surface() == CadSurfaceKind::Pcb {
        if selection.kind == ElectronicsSelectionKind::Trace {
            return editor
                .pcb()
                .traces
                .iter()
                .enumerate()
                .find(|(_, trace)| trace.id == selection.source_id)
                .map(|(index, trace)| ElectronicsInspectorData {
                    title: format!("Trace #{}", index + 1),
                    fields: vec![
                        ("Value".to_string(), trace.net.clone()),
                        ("Category".to_string(), "PCB trace".to_string()),
                        (
                            "Position".to_string(),
                            format!("{} points", trace.points.len()),
                        ),
                        ("Rotation".to_string(), "Orthogonal".to_string()),
                        (
                            "Footprint".to_string(),
                            trace.layer.display_name().to_string(),
                        ),
                        ("Editable".to_string(), "false".to_string()),
                    ],
                    pins: Vec::new(),
                });
        }

        return editor
            .pcb()
            .components
            .iter()
            .find(|component| component.component_id == selection.source_id)
            .map(|component| ElectronicsInspectorData {
                title: format!("{}  {}", component.designator, component.value),
                fields: vec![
                    ("Value".to_string(), component.value.clone()),
                    ("Category".to_string(), "PCB component".to_string()),
                    (
                        "Position".to_string(),
                        format!("{:.1}, {:.1}", component.position.x, component.position.y),
                    ),
                    (
                        "Rotation".to_string(),
                        format!("{:.0} deg", component.rotation),
                    ),
                    ("Footprint".to_string(), component.footprint.clone()),
                    ("Editable".to_string(), (!component.locked).to_string()),
                ],
                pins: component
                    .pad_nets
                    .iter()
                    .enumerate()
                    .map(|(index, net)| (format!("Pad {}", index + 1), net.clone()))
                    .collect(),
            });
    }

    if selection.kind == ElectronicsSelectionKind::Wire {
        return editor
            .schematic()
            .wires
            .iter()
            .enumerate()
            .find(|(_, wire)| wire.id == selection.source_id)
            .map(|(index, wire)| ElectronicsInspectorData {
                title: format!("Wire #{}", index + 1),
                fields: vec![
                    ("Value".to_string(), wire.net.clone()),
                    ("Identity label".to_string(), "Net".to_string()),
                    ("Editable".to_string(), "false".to_string()),
                    ("Category".to_string(), "Connection".to_string()),
                    (
                        "Position".to_string(),
                        format!(
                            "{:.1}, {:.1} -> {:.1}, {:.1}",
                            wire.start.x, wire.start.y, wire.end.x, wire.end.y
                        ),
                    ),
                    ("Rotation".to_string(), "Orthogonal".to_string()),
                    ("Footprint".to_string(), "Orthogonal path".to_string()),
                ],
                pins: Vec::new(),
            });
    }

    let (component_index, component) = editor
        .schematic()
        .components
        .iter()
        .enumerate()
        .find(|(_, component)| component.id == selection.source_id)?;
    let netlist = editor.schematic().netlist();
    Some(ElectronicsInspectorData {
        title: format!("{}  {}", component.designator, component.kind_label()),
        fields: vec![
            ("Value".to_string(), component.value.clone()),
            ("Category".to_string(), component.category.clone()),
            (
                "Position".to_string(),
                format!("{:.1}, {:.1}", component.position.x, component.position.y),
            ),
            (
                "Rotation".to_string(),
                format!("{:.0} deg", component.rotation),
            ),
            ("Footprint".to_string(), component.footprint.clone()),
        ],
        pins: component
            .pins
            .iter()
            .enumerate()
            .map(|(pin_index, pin)| {
                (
                    pin.name.clone(),
                    netlist
                        .net_for_pin(component_index, pin_index)
                        .map(|net| net.name.clone())
                        .unwrap_or_default(),
                )
            })
            .collect(),
    })
}

pub(crate) struct ElectronicsWorkbenchData {
    pub schematic_name: String,
    pub counts: (usize, usize, usize),
    pub components: Vec<ElectronicsNavigatorEntry>,
    pub wires: Vec<ElectronicsNavigatorEntry>,
    pub library: Vec<ElectronicsLibraryEntry>,
}

pub(crate) fn analysis_lines(
    editor: &NativeElectronicsEditor,
    panel: ElectronicsAnalysisPanel,
    language: Language,
) -> Vec<ElectronicsAnalysisLine> {
    let label = |key: &str| i18n::t(key, language);
    let metric = |key: &str, value: usize| {
        ElectronicsAnalysisLine::normal(format!("{}: {value}", label(key)))
    };
    let mut lines = Vec::new();
    match panel {
        ElectronicsAnalysisPanel::Drc => {
            lines.push(metric(
                "electronics.analysis.components",
                editor.schematic().components.len(),
            ));
            lines.push(metric(
                "electronics.analysis.wires",
                editor.schematic().wires.len(),
            ));
            lines.push(metric(
                "electronics.analysis.nets",
                editor.schematic().netlist().nets.len(),
            ));
            if editor.analysis_running() {
                lines.push(ElectronicsAnalysisLine::with_tone(
                    format!(
                        "{}: {}",
                        label("electronics.analysis.status"),
                        label("electronics.analysis.running")
                    ),
                    ElectronicsAnalysisTone::Running,
                ));
                lines.push(ElectronicsAnalysisLine::with_tone(
                    label("electronics.analysis.drc_running"),
                    ElectronicsAnalysisTone::Running,
                ));
            } else if let Some(report) = editor.drc_report() {
                let tone = if report.passed() {
                    ElectronicsAnalysisTone::Passed
                } else {
                    ElectronicsAnalysisTone::Issues
                };
                let result = if report.passed() {
                    label("electronics.analysis.passed")
                } else {
                    label("electronics.analysis.issues_found")
                };
                lines.push(ElectronicsAnalysisLine::with_tone(
                    format!(
                        "{}: {} | {}: {} | {}: {} | {}: {}",
                        label("electronics.analysis.status"),
                        result,
                        label("electronics.analysis.errors"),
                        report.errors.len(),
                        label("electronics.analysis.warnings"),
                        report.warnings.len(),
                        label("electronics.analysis.info"),
                        report.info.len(),
                    ),
                    tone,
                ));
            } else {
                lines.push(ElectronicsAnalysisLine::with_tone(
                    format!(
                        "{}: {}",
                        label("electronics.analysis.status"),
                        label("electronics.analysis.not_run")
                    ),
                    ElectronicsAnalysisTone::Normal,
                ));
            }
            if editor.drc_is_stale() && editor.drc_report().is_some() {
                lines.push(ElectronicsAnalysisLine::with_tone(
                    label("electronics.analysis.drc_stale"),
                    ElectronicsAnalysisTone::Issues,
                ));
            }
            if editor.drc_lines().is_empty() {
                lines.push(ElectronicsAnalysisLine::normal(label(
                    "electronics.analysis.run_design_check",
                )));
            } else {
                lines.extend(editor.drc_lines().iter().map(report_tone));
            }
        }
        ElectronicsAnalysisPanel::Simulation => {
            lines.push(metric(
                "electronics.analysis.components",
                editor.schematic().components.len(),
            ));
            lines.push(metric(
                "electronics.analysis.nets",
                editor.schematic().netlist().nets.len(),
            ));
            if editor.analysis_running() {
                lines.push(ElectronicsAnalysisLine::with_tone(
                    format!(
                        "{}: {}",
                        label("electronics.analysis.status"),
                        label("electronics.analysis.running")
                    ),
                    ElectronicsAnalysisTone::Running,
                ));
                lines.push(ElectronicsAnalysisLine::with_tone(
                    label("electronics.analysis.simulation_running"),
                    ElectronicsAnalysisTone::Running,
                ));
            } else if editor.simulation_lines().is_empty() {
                lines.push(ElectronicsAnalysisLine::with_tone(
                    format!(
                        "{}: {}",
                        label("electronics.analysis.status"),
                        label("electronics.analysis.not_run")
                    ),
                    ElectronicsAnalysisTone::Normal,
                ));
                lines.push(ElectronicsAnalysisLine::normal(label(
                    "electronics.analysis.run_dc_solver",
                )));
            } else {
                if editor.simulation_is_stale() {
                    lines.push(ElectronicsAnalysisLine::with_tone(
                        label("electronics.analysis.simulation_stale"),
                        ElectronicsAnalysisTone::Issues,
                    ));
                }
                lines.extend(editor.simulation_lines().iter().map(report_tone));
            }
        }
    }
    lines
}

/// Maps the structured line kind onto the dock tone.
///
/// Classification is never derived from the rendered text: an `[ERROR]
/// short_circuit:` line used to fall through to gray because it contained none of
/// the English words the dock searched for.
fn report_tone(
    line: &crate::electronics_controller::ElectronicsReportLine,
) -> ElectronicsAnalysisLine {
    let tone = match line.kind {
        ElectronicsReportLineKind::Status
        | ElectronicsReportLineKind::Message
        | ElectronicsReportLineKind::Info => ElectronicsAnalysisTone::Normal,
        ElectronicsReportLineKind::Running => ElectronicsAnalysisTone::Running,
        ElectronicsReportLineKind::Passed => ElectronicsAnalysisTone::Passed,
        ElectronicsReportLineKind::Issues | ElectronicsReportLineKind::Warning => {
            ElectronicsAnalysisTone::Issues
        }
        ElectronicsReportLineKind::Failed | ElectronicsReportLineKind::Error => {
            ElectronicsAnalysisTone::Failed
        }
    };
    // A finding that names a component becomes a navigation target, so clicking
    // the row selects and reveals it instead of leaving the user to hunt for it.
    match line.target {
        Some(source_id) => ElectronicsAnalysisLine::with_target(
            line.text.clone(),
            tone,
            crate::panels::electronics_surface::ElectronicsAnalysisTarget::Component { source_id },
        ),
        None => ElectronicsAnalysisLine::with_tone(line.text.clone(), tone),
    }
}

/// Workbench-level notices the dock and the status bar must show. Kept separate
/// from the analysis lines so a failure is never presented as a design result.
pub(crate) fn notice_lines(editor: &NativeElectronicsEditor, language: Language) -> Vec<String> {
    let mut notices = Vec::new();
    if editor.sync_is_stale() {
        notices.push(i18n::t("electronics.sync.pcb_stale", language));
    }
    notices.extend(editor.surface_errors().iter().cloned());
    notices
}

impl ElectronicsWorkbenchData {
    pub fn from_editor(editor: Option<&NativeElectronicsEditor>) -> Self {
        let Some(editor) = editor else {
            return Self {
                schematic_name: "Main Schematic".to_string(),
                counts: (0, 0, 0),
                components: Vec::new(),
                wires: Vec::new(),
                library: Vec::new(),
            };
        };

        let components = if editor.active_surface() == CadSurfaceKind::Pcb {
            editor
                .pcb()
                .components
                .iter()
                .enumerate()
                .map(|(index, component)| ElectronicsNavigatorEntry {
                    label: format!("{}  {}", component.designator, component.value),
                    secondary: format!(
                        "{} / {}",
                        component.footprint,
                        component.layer.display_name()
                    ),
                    secondary_key: None,
                    command: format!("electronics.navigator.select.{index}"),
                    icon: electronics_icon_for_category("pcb"),
                    active: editor.selection().is_some_and(|selection| {
                        matches!(
                            selection.kind,
                            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
                        ) && selection.source_id == component.component_id
                    }),
                })
                .collect()
        } else {
            editor
                .schematic()
                .components
                .iter()
                .enumerate()
                .map(|(index, component)| ElectronicsNavigatorEntry {
                    label: format!("{}  {}", component.designator, component.value),
                    secondary: component.category.clone(),
                    secondary_key: None,
                    command: format!("electronics.navigator.select.{index}"),
                    icon: electronics_icon_for_category(&component.category),
                    active: editor.selection().is_some_and(|selection| {
                        selection.kind == ElectronicsSelectionKind::Component
                            && selection.source_id == component.id
                    }),
                })
                .collect()
        };

        let wires = if editor.active_surface() == CadSurfaceKind::Pcb {
            editor
                .pcb()
                .traces
                .iter()
                .enumerate()
                .map(|(index, trace)| ElectronicsNavigatorEntry {
                    label: format!("Trace #{}", index + 1),
                    secondary: trace.net.clone(),
                    secondary_key: None,
                    command: format!("electronics.navigator.select-trace.{index}"),
                    icon: UiIconId::Move,
                    active: editor.selection().is_some_and(|selection| {
                        selection.kind == ElectronicsSelectionKind::Trace
                            && selection.source_id == trace.id
                    }),
                })
                .collect()
        } else {
            editor
                .schematic()
                .wires
                .iter()
                .enumerate()
                .map(|(index, wire)| ElectronicsNavigatorEntry {
                    label: format!("Wire #{}", index + 1),
                    secondary: wire.net.clone(),
                    secondary_key: None,
                    command: format!("electronics.navigator.select-wire.{index}"),
                    icon: UiIconId::Move,
                    active: editor.selection().is_some_and(|selection| {
                        selection.kind == ElectronicsSelectionKind::Wire
                            && selection.source_id == wire.id
                    }),
                })
                .collect()
        };

        let library = editor
            .library()
            .components
            .iter()
            .enumerate()
            .map(|(index, template)| ElectronicsLibraryEntry {
                index,
                name: template.name.clone(),
                category: template.category.clone(),
                description: template.description.clone(),
                favorite: template.favorite,
                icon: electronics_icon_for_category(&template.category),
                image_key: template
                    .icon_asset
                    .map(|asset| format!("electronics://{asset}")),
            })
            .collect();

        let counts = if editor.active_surface() == CadSurfaceKind::Pcb {
            let mut net_names = HashSet::new();
            net_names.extend(editor.pcb().traces.iter().map(|trace| trace.net.as_str()));
            net_names.extend(
                editor
                    .pcb()
                    .airwires
                    .iter()
                    .map(|airwire| airwire.net.as_str()),
            );
            (
                editor.pcb().components.len(),
                editor.pcb().traces.len(),
                net_names.len(),
            )
        } else {
            let nets = editor
                .schematic()
                .wires
                .iter()
                .map(|wire| wire.net.as_str())
                .collect::<HashSet<_>>()
                .len();
            (
                editor.schematic().components.len(),
                editor.schematic().wires.len(),
                nets,
            )
        };

        Self {
            schematic_name: editor.schematic().name.clone(),
            counts,
            components,
            wires,
            library,
        }
    }
}

fn electronics_icon_for_category(category: &str) -> UiIconId {
    match category.to_ascii_lowercase().as_str() {
        "pcb" => UiIconId::Pcb,
        "passive" => UiIconId::Scale,
        "diode" => UiIconId::Warning,
        "power" => UiIconId::Add,
        "magnet" => UiIconId::Move,
        _ => UiIconId::Schematic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::electronics_controller::{ElectronicsReportLine, NativeElectronicsEditor};

    #[test]
    fn analysis_metrics_follow_the_requested_locale() {
        let editor = NativeElectronicsEditor::empty("Test");
        let english = analysis_lines(&editor, ElectronicsAnalysisPanel::Drc, Language::English);
        let spanish = analysis_lines(&editor, ElectronicsAnalysisPanel::Drc, Language::Spanish);

        assert!(english[0].text.starts_with("Components:"));
        assert!(spanish[0].text.starts_with("Componentes:"));
        assert_ne!(english[0].text, spanish[0].text);
    }

    #[test]
    fn a_stale_drc_result_is_still_listed_with_an_obsolete_notice() {
        let mut editor = NativeElectronicsEditor::empty("Test");
        editor.run_drc();
        editor
            .schematic
            .add_component(raf_electronics::component::ElectronicComponent::resistor(
                "10k",
            ));
        editor.rebuild_scene();

        let lines = analysis_lines(&editor, ElectronicsAnalysisPanel::Drc, Language::English);
        assert!(lines
            .iter()
            .any(|line| line.text == "Outdated: the schematic changed after this check."));
        assert!(lines.iter().any(|line| line.tone
            == crate::panels::electronics_surface::ElectronicsAnalysisTone::Passed
            || line.tone == crate::panels::electronics_surface::ElectronicsAnalysisTone::Issues));
    }

    #[test]
    fn line_tone_comes_from_the_structured_kind_not_from_the_text() {
        let error = ElectronicsReportLine {
            kind: ElectronicsReportLineKind::Error,
            target: None,
            text: "[ERROR] short_circuit: N001 shorts VCC to GND".to_string(),
        };
        assert_eq!(
            report_tone(&error).tone,
            crate::panels::electronics_surface::ElectronicsAnalysisTone::Failed
        );

        let neutral = ElectronicsReportLine {
            kind: ElectronicsReportLineKind::Info,
            target: None,
            text: "issues: nothing to report".to_string(),
        };
        assert_eq!(
            report_tone(&neutral).tone,
            crate::panels::electronics_surface::ElectronicsAnalysisTone::Normal
        );
    }
}
