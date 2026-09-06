//! Electronics-specific workbench model data.
//!
//! The shell owns layout and intent dispatch. This module owns only the
//! read-only projections used by the navigator and inspector, keeping
//! Electronics document/catalog logic out of the large workbench compositor.

use std::collections::HashSet;

use crate::electronics_controller::{ElectronicsSelectionKind, NativeElectronicsEditor};
use crate::panels::electronics_navigator_surface::{
    ElectronicsLibraryEntry, ElectronicsNavigatorEntry,
};
use crate::panels::electronics_surface::{ElectronicsAnalysisLine, ElectronicsAnalysisTone};
use raf_core::{i18n, Language};
use raf_electronics::CadSurfaceKind;
use raf_render::api_graphic_basic::ui_surface::UiIconId;

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
    tab: &str,
    language: Language,
) -> Vec<ElectronicsAnalysisLine> {
    let label = |key: &str| i18n::t(key, language);
    let metric = |key: &str, value: usize| {
        ElectronicsAnalysisLine::normal(format!("{}: {value}", label(key)))
    };
    match tab {
        "drc" => {
            let mut lines = vec![
                metric(
                    "electronics.analysis.components",
                    editor.schematic().components.len(),
                ),
                metric("electronics.analysis.wires", editor.schematic().wires.len()),
                metric(
                    "electronics.analysis.nets",
                    editor.schematic().netlist().nets.len(),
                ),
            ];
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
            if editor.drc_lines().is_empty() {
                lines.push(ElectronicsAnalysisLine::normal(label(
                    "electronics.analysis.run_design_check",
                )));
            } else {
                lines.extend(editor.drc_lines().iter().cloned().map(classify_report_line));
            }
            lines
        }
        "simulation" => {
            let mut lines = vec![
                metric(
                    "electronics.analysis.components",
                    editor.schematic().components.len(),
                ),
                metric(
                    "electronics.analysis.nets",
                    editor.schematic().netlist().nets.len(),
                ),
            ];
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
                lines.extend(
                    editor
                        .simulation_lines()
                        .iter()
                        .cloned()
                        .map(classify_report_line),
                );
            }
            lines
        }
        _ => vec![ElectronicsAnalysisLine::normal(label(
            "electronics.analysis.unavailable",
        ))],
    }
}

fn classify_report_line(text: String) -> ElectronicsAnalysisLine {
    let normalized = text.to_ascii_lowercase();
    let tone = if normalized.contains("running") {
        ElectronicsAnalysisTone::Running
    } else if normalized.contains("passed") || normalized.contains("converged") {
        ElectronicsAnalysisTone::Passed
    } else if normalized.contains("issues") || normalized.contains("not converged") {
        ElectronicsAnalysisTone::Issues
    } else if normalized.contains("cancelled") || normalized.contains("failed") {
        ElectronicsAnalysisTone::Failed
    } else {
        ElectronicsAnalysisTone::Normal
    };
    ElectronicsAnalysisLine::with_tone(text, tone)
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

    #[test]
    fn analysis_metrics_follow_the_requested_locale() {
        let editor = NativeElectronicsEditor::empty("Test");
        let english = analysis_lines(&editor, "drc", Language::English);
        let spanish = analysis_lines(&editor, "drc", Language::Spanish);

        assert!(english[0].text.starts_with("Components:"));
        assert!(spanish[0].text.starts_with("Componentes:"));
        assert_ne!(english[0].text, spanish[0].text);
    }
}
