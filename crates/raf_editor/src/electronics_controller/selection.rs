//! Selection identity, hover and navigation for the live document.

use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElectronicsSelectionKind {
    Component,
    Pin,
    Wire,
    Trace,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ElectronicsSelection {
    pub source_id: Uuid,
    pub kind: ElectronicsSelectionKind,
}

impl NativeElectronicsEditor {
    /// Pointer hover target. Surfaces use it for a hover affordance without
    /// disturbing the committed selection.
    pub fn hovered(&self) -> Option<&ElectronicsSelection> {
        self.hovered.as_ref()
    }

    /// Stable model identities the CAD surface should present as hovered.
    pub fn hovered_source_ids(&self) -> Vec<[u8; 16]> {
        self.hovered
            .map(|hovered| vec![*hovered.source_id.as_bytes()])
            .unwrap_or_default()
    }

    /// Net index currently focused for highlighting, matching `CadObject::net_id`.
    pub fn highlight_net(&self) -> Option<usize> {
        self.highlight_net
    }

    /// Orientation belongs to the document currently being edited.
    pub fn rotation_degrees(&self, source_id: Uuid) -> f32 {
        match self.active_surface {
            CadSurfaceKind::Schematic => self
                .schematic
                .components
                .iter()
                .find(|c| c.id == source_id)
                .map(|c| c.rotation),
            CadSurfaceKind::Pcb => self
                .pcb
                .components
                .iter()
                .find(|c| c.component_id == source_id)
                .map(|c| c.rotation),
        }
        .unwrap_or(0.0)
    }

    /// Records the hover target from the CAD input layer. The input layer owns the
    /// pointer gesture; the controller only remembers the result so a surface can
    /// present it.
    pub fn set_hovered(&mut self, hover: Option<ElectronicsSelection>) {
        if self.hovered == hover {
            return;
        }
        self.hovered = hover;
        self.touch();
    }

    /// Records the focused net from the CAD input layer.
    pub fn set_highlight_net(&mut self, net: Option<usize>) {
        if self.highlight_net == net {
            return;
        }
        self.highlight_net = net;
        self.touch();
    }

    pub fn selection(&self) -> Option<ElectronicsSelection> {
        self.selection
    }

    /// True when the current selection still resolves to something the delete
    /// path can remove, so a confirmation is never armed for nothing.
    pub(super) fn delete_target_exists(&self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        match selection.kind {
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin => {
                match self.active_surface {
                    CadSurfaceKind::Schematic => self
                        .schematic
                        .components
                        .iter()
                        .any(|component| component.id == selection.source_id),
                    CadSurfaceKind::Pcb => self
                        .pcb
                        .components
                        .iter()
                        .any(|component| component.component_id == selection.source_id),
                }
            }
            ElectronicsSelectionKind::Wire => self
                .schematic
                .wires
                .iter()
                .any(|wire| wire.id == selection.source_id),
            ElectronicsSelectionKind::Trace => {
                self.active_surface == CadSurfaceKind::Pcb
                    && self
                        .pcb
                        .traces
                        .iter()
                        .any(|trace| trace.id == selection.source_id)
            }
            ElectronicsSelectionKind::Other => false,
        }
    }

    pub fn select_component(&mut self, index: usize) -> bool {
        if self.active_surface == CadSurfaceKind::Pcb {
            let Some(component) = self.pcb.components.get(index) else {
                return false;
            };
            self.selection = Some(ElectronicsSelection {
                source_id: component.component_id,
                kind: ElectronicsSelectionKind::Component,
            });
            self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
                object_id: format!("pcb_component:{}", component.component_id),
                source_id: Some(component.component_id),
                kind: CadObjectKind::Component,
                layer: match component.layer {
                    raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                    raf_electronics::PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
                },
            });
            self.touch_ui();
            return true;
        }
        let Some(component) = self.schematic.components.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: component.id,
            kind: ElectronicsSelectionKind::Component,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("component:{}", component.id),
            source_id: Some(component.id),
            kind: CadObjectKind::Component,
            layer: CadLayerKind::Schematic,
        });
        self.touch_ui();
        true
    }

    pub fn select_wire(&mut self, index: usize) -> bool {
        let Some(wire) = self.schematic.wires.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: wire.id,
            kind: ElectronicsSelectionKind::Wire,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("wire:{}", wire.id),
            source_id: Some(wire.id),
            kind: CadObjectKind::Wire,
            layer: CadLayerKind::Schematic,
        });
        self.touch_ui();
        true
    }

    pub fn select_trace(&mut self, index: usize) -> bool {
        if self.active_surface != CadSurfaceKind::Pcb {
            return false;
        }
        let Some(trace) = self.pcb.traces.get(index) else {
            return false;
        };
        self.selection = Some(ElectronicsSelection {
            source_id: trace.id,
            kind: ElectronicsSelectionKind::Trace,
        });
        self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
            object_id: format!("trace:{}", trace.id),
            source_id: Some(trace.id),
            kind: CadObjectKind::Trace,
            layer: match trace.layer {
                raf_electronics::PcbLayer::TopCopper => CadLayerKind::PcbTopCopper,
                raf_electronics::PcbLayer::BottomCopper => CadLayerKind::PcbBottomCopper,
            },
        });
        self.touch_ui();
        true
    }

    pub fn can_rotate_selection(&self) -> bool {
        let Some(selection) = self.selection else {
            return false;
        };
        if !matches!(
            selection.kind,
            ElectronicsSelectionKind::Component | ElectronicsSelectionKind::Pin
        ) {
            return false;
        }
        match self.active_surface {
            CadSurfaceKind::Schematic => self
                .schematic
                .components
                .iter()
                .any(|component| component.id == selection.source_id && !component.locked),
            CadSurfaceKind::Pcb => self.pcb.components.iter().any(|component| {
                component.component_id == selection.source_id && !component.locked
            }),
        }
    }

    /// Selects the component behind an analysis finding.
    ///
    /// `panel` selects which report to read and `index` the finding's position
    /// inside it. Resolution is by the identity carried on the report row, not
    /// by a row offset, so adding a status line to the dock can never redirect a
    /// click to the wrong component. Returns `false` when the finding has no
    /// component, or the component no longer exists in the document.
    pub fn focus_analysis_issue(&mut self, panel: &str, index: usize) -> bool {
        let source = match panel {
            "drc" => self.drc_lines.get(index),
            "simulation" => self.simulation_lines.get(index),
            _ => None,
        };
        let Some(target) = source.and_then(|line| line.target) else {
            return false;
        };
        self.highlight_net = None;
        if !self.select_component_by_id(target) {
            return false;
        }
        self.recenter_on_selection();
        true
    }

    /// Selects a component by identity rather than by list position.
    pub fn select_component_by_id(&mut self, source_id: Uuid) -> bool {
        if self.active_surface == CadSurfaceKind::Pcb {
            if self.pcb.vias.iter().any(|via| via.id == source_id) {
                self.selection = Some(ElectronicsSelection {
                    source_id,
                    kind: ElectronicsSelectionKind::Pin,
                });
                self.interaction.selected = Some(raf_electronics::cad_interaction::CadSelection {
                    object_id: format!("via:{source_id}:top"),
                    source_id: Some(source_id),
                    kind: CadObjectKind::Pad,
                    layer: CadLayerKind::PcbTopCopper,
                });
                self.touch_ui();
                return true;
            }
            if let Some(index) = self
                .pcb
                .traces
                .iter()
                .position(|trace| trace.id == source_id)
            {
                return self.select_trace(index);
            }
            let Some(index) = self
                .pcb
                .components
                .iter()
                .position(|component| component.component_id == source_id)
            else {
                return false;
            };
            return self.select_component(index);
        }
        let Some(index) = self
            .schematic
            .components
            .iter()
            .position(|component| component.id == source_id)
        else {
            return false;
        };
        self.select_component(index)
    }

    /// Moves the camera to the current selection without changing zoom.
    ///
    /// Navigation, not editing: it records no history and leaves the project
    /// clean.
    pub fn recenter_on_selection(&mut self) {
        let Some(selection) = self.selection else {
            return;
        };
        let position = if self.active_surface == CadSurfaceKind::Pcb {
            self.pcb
                .components
                .iter()
                .find(|component| component.component_id == selection.source_id)
                .map(|component| component.position)
                .or_else(|| {
                    self.pcb
                        .vias
                        .iter()
                        .find(|via| via.id == selection.source_id)
                        .map(|via| via.position)
                })
                .or_else(|| {
                    self.pcb
                        .traces
                        .iter()
                        .find(|trace| trace.id == selection.source_id)
                        .and_then(|trace| trace.points.first().copied())
                })
        } else {
            self.schematic
                .components
                .iter()
                .find(|component| component.id == selection.source_id)
                .map(|component| component.position)
        };
        let Some(position) = position else {
            return;
        };
        self.camera.center = position;
        if self.active_surface == CadSurfaceKind::Pcb {
            if let Some(placement) = self.pcb.components.iter().find(|c| {
                c.component_id == selection.source_id
                    && self.pcb.footprint_assets.contains_key(&c.footprint)
            }) {
                let span = self
                    .pcb
                    .resolved_footprint(placement)
                    .body_size
                    .max_element()
                    .max(0.1);
                self.camera.zoom = self
                    .camera
                    .zoom
                    .max((180.0 / span).clamp(MIN_ZOOM, MAX_ZOOM));
                self.camera_initialized = true;
            }
        }
        self.touch();
    }
}
