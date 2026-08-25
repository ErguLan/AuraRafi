//! Command-bus adapter for the native Electronics document.
//!
//! Kept outside the controller so command protocol plumbing does not grow
//! inside the CAD input/camera module. The child implementation still has
//! access to the controller's private document state through its module
//! boundary; transports only see the public `execute_catalog_command` method.

use super::*;

impl NativeElectronicsEditor {
    /// Executes a catalog Electronics/PCB command against the live native
    /// document. Transport adapters (CLI, MCP and Agent) call this method so
    /// they cannot mutate a detached schematic copy or bypass editor history.
    pub fn execute_catalog_command(
        &mut self,
        command_name: &str,
        command: &crate::commands::parser::ParsedCommand,
    ) -> crate::commands::output::CommandOutput {
        let before = self.snapshot();
        let selected_component = self.selection.and_then(|selection| {
            matches!(
                selection.kind,
                ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
            )
            .then(|| {
                if self.active_surface == CadSurfaceKind::Pcb {
                    self.pcb
                        .components
                        .iter()
                        .position(|component| component.component_id == selection.source_id)
                } else {
                    self.schematic
                        .components
                        .iter()
                        .position(|component| component.id == selection.source_id)
                }
            })
            .flatten()
        });
        let selected_wire = self
            .selection
            .and_then(|selection| {
                (selection.kind == ElectronicsSelectionKind::Wire).then(|| {
                    self.schematic
                        .wires
                        .iter()
                        .position(|wire| wire.id == selection.source_id)
                })
            })
            .flatten();
        let selected_trace = self
            .selection
            .and_then(|selection| {
                (selection.kind == ElectronicsSelectionKind::Trace).then(|| {
                    self.pcb
                        .traces
                        .iter()
                        .position(|trace| trace.id == selection.source_id)
                })
            })
            .flatten();

        let mut context = crate::commands::electronics::ElectronicsCommandContext {
            schematic_view: crate::commands::electronics::SchematicCommandView {
                schematic: &mut self.schematic,
                selected_component: if self.active_surface == CadSurfaceKind::Pcb {
                    None
                } else {
                    selected_component
                },
                selected_wire,
            },
            pcb_view: crate::commands::electronics::PcbCommandView {
                layout: &mut self.pcb,
                selected_component: if self.active_surface == CadSurfaceKind::Pcb {
                    selected_component
                } else {
                    None
                },
                selected_trace,
            },
        };
        let mut output = crate::commands::electronics::execute(command_name, command, &mut context);
        let schematic_component = context.schematic_view.selected_component;
        let schematic_wire = context.schematic_view.selected_wire;
        let pcb_component = context.pcb_view.selected_component;
        let pcb_trace = context.pcb_view.selected_trace;
        drop(context);

        if matches!(command_name, "electronics.drc" | "electronics.diagnose") {
            self.run_drc();
            output.changed = true;
        }
        if matches!(
            command_name,
            "electronics.simulate" | "electronics.diagnose"
        ) {
            self.run_simulation();
            output.changed = true;
        }

        let current = self.snapshot();
        if output.changed && self.history.record(before, &current) {
            self.dirty = true;
        }
        if output.changed && !matches!(command_name, "electronics.drc" | "electronics.diagnose") {
            self.rebuild_scene();
        }

        self.apply_catalog_selection(
            schematic_component,
            schematic_wire,
            pcb_component,
            pcb_trace,
        );
        if output.changed {
            self.touch_ui();
        }
        output
    }

    fn apply_catalog_selection(
        &mut self,
        schematic_component: Option<usize>,
        schematic_wire: Option<usize>,
        pcb_component: Option<usize>,
        pcb_trace: Option<usize>,
    ) {
        let selection = if self.active_surface == CadSurfaceKind::Pcb {
            pcb_trace
                .and_then(|index| {
                    self.pcb
                        .traces
                        .get(index)
                        .map(|trace| (trace.id, ElectronicsSelectionKind::Trace))
                })
                .or_else(|| {
                    pcb_component.and_then(|index| {
                        self.pcb.components.get(index).map(|component| {
                            (component.component_id, ElectronicsSelectionKind::Component)
                        })
                    })
                })
        } else {
            schematic_wire
                .and_then(|index| {
                    self.schematic
                        .wires
                        .get(index)
                        .map(|wire| (wire.id, ElectronicsSelectionKind::Wire))
                })
                .or_else(|| {
                    schematic_component.and_then(|index| {
                        self.schematic
                            .components
                            .get(index)
                            .map(|component| (component.id, ElectronicsSelectionKind::Component))
                    })
                })
        };
        self.selection =
            selection.map(|(source_id, kind)| ElectronicsSelection { source_id, kind });
        if let Some(selection) = self.selection {
            self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
                object_id: match selection.kind {
                    ElectronicsSelectionKind::Component => {
                        if self.active_surface == CadSurfaceKind::Pcb {
                            format!("pcb_component:{}", selection.source_id)
                        } else {
                            format!("component:{}", selection.source_id)
                        }
                    }
                    ElectronicsSelectionKind::Wire => format!("wire:{}", selection.source_id),
                    ElectronicsSelectionKind::Trace => format!("trace:{}", selection.source_id),
                    _ => String::new(),
                },
                source_id: Some(selection.source_id),
                kind: match selection.kind {
                    ElectronicsSelectionKind::Component => CadObjectKind::Component,
                    ElectronicsSelectionKind::Wire => CadObjectKind::Wire,
                    ElectronicsSelectionKind::Trace => CadObjectKind::Trace,
                    _ => CadObjectKind::Component,
                },
                layer: if self.active_surface == CadSurfaceKind::Pcb {
                    match selection.kind {
                        ElectronicsSelectionKind::Trace => self
                            .pcb
                            .traces
                            .iter()
                            .find(|trace| trace.id == selection.source_id)
                            .map(|trace| match trace.layer {
                                raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                                raf_electronics::PcbLayer::BottomCopper => {
                                    CadLayerKind::PcbBottomCopper
                                }
                            })
                            .unwrap_or(CadLayerKind::PcbTopCopper),
                        _ => CadLayerKind::PcbTopCopper,
                    }
                } else {
                    CadLayerKind::Schematic
                },
            });
        } else {
            self.interaction.clear_selection();
        }
    }
}
