//! Independent schematic graph for electronics.
//!
//! Separates electronics data from the game SceneGraph.
//! This prevents the "Anthem/Frostbite" problem: forcing a game engine's
//! data model to support an unrelated domain (electronics design).
//!
//! The SchematicGraph owns components, wires, nets, and simulation state.
//! It does NOT depend on raf_core::SceneGraph at all.
//!
//! Lightweight: just Vecs with UUIDs, zero external allocations.

use glam::Vec2;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::component::ElectronicComponent;
use crate::schematic::Wire;

// ---------------------------------------------------------------------------
// Schematic Node (component placement on the canvas)
// ---------------------------------------------------------------------------

/// Unique identifier for a schematic element.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SchematicNodeId(pub usize);

/// A placed component in the schematic canvas with layout info.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicNode {
    /// Unique ID for this placement.
    pub id: Uuid,
    /// The electronic component data.
    pub component: ElectronicComponent,
    /// Position on the schematic canvas (pixels).
    pub canvas_position: Vec2,
    /// Rotation in degrees (0, 90, 180, 270).
    pub rotation_deg: f32,
    /// Whether this node is selected in the editor.
    #[serde(skip)]
    pub selected: bool,
    /// Whether this node is visible.
    pub visible: bool,
    /// Layer (for multi-sheet schematics, future).
    pub layer: u8,
}

impl SchematicNode {
    pub fn new(component: ElectronicComponent, position: Vec2) -> Self {
        Self {
            id: component.id,
            component,
            canvas_position: position,
            rotation_deg: 0.0,
            selected: false,
            visible: true,
            layer: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// Net (electrical connection between pins)
// ---------------------------------------------------------------------------

/// A net represents a single electrical connection (e.g., "VCC", "GND", "N001").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Net {
    /// Net name (auto-generated or user-defined).
    pub name: String,
    /// Pin references that belong to this net.
    /// Each entry is (component_index, pin_index).
    pub pins: Vec<(usize, usize)>,
    /// Computed voltage from simulation (if available).
    pub voltage: Option<f64>,
}

// ---------------------------------------------------------------------------
// Schematic Graph
// ---------------------------------------------------------------------------

/// Independent data graph for electronics schematics.
/// Owns all schematic state, completely separate from game SceneGraph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchematicGraph {
    /// All placed components.
    nodes: Vec<SchematicNode>,
    /// All wires (visual connections on canvas).
    wires: Vec<Wire>,
    /// Computed nets (built from wires + pin positions).
    nets: Vec<Net>,
    /// Schematic name.
    pub name: String,
    /// Grid spacing for snap-to-grid.
    pub grid_spacing: f32,
    /// Canvas offset (pan position).
    #[serde(skip)]
    pub canvas_offset: Vec2,
    /// Canvas zoom level.
    #[serde(skip)]
    pub canvas_zoom: f32,
    /// Whether the schematic has unsaved changes.
    #[serde(skip)]
    pub modified: bool,
}

impl SchematicGraph {
    /// Create an empty schematic graph.
    pub fn new(name: &str) -> Self {
        Self {
            nodes: Vec::new(),
            wires: Vec::new(),
            nets: Vec::new(),
            name: name.to_string(),
            grid_spacing: 20.0,
            canvas_offset: Vec2::ZERO,
            canvas_zoom: 1.0,
            modified: false,
        }
    }

    // -------------------------------------------------------------------
    // Component operations
    // -------------------------------------------------------------------

    /// Add a component at a position. Returns its index.
    pub fn add_component(&mut self, component: ElectronicComponent, position: Vec2) -> SchematicNodeId {
        let id = SchematicNodeId(self.nodes.len());
        self.nodes.push(SchematicNode::new(component, position));
        self.modified = true;
        id
    }

    /// Remove a component by index (swap-remove for performance).
    pub fn remove_component(&mut self, idx: usize) -> bool {
        if idx < self.nodes.len() {
            self.nodes.remove(idx);
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// Get a component reference.
    pub fn get_component(&self, idx: usize) -> Option<&SchematicNode> {
        self.nodes.get(idx)
    }

    /// Get a mutable component reference.
    pub fn get_component_mut(&mut self, idx: usize) -> Option<&mut SchematicNode> {
        self.nodes.get_mut(idx)
    }

    /// Duplicate a component at the given index with a position offset.
    pub fn duplicate_component(&mut self, idx: usize, offset: Vec2) -> Option<SchematicNodeId> {
        let node = self.nodes.get(idx)?.clone();
        let new_id = SchematicNodeId(self.nodes.len());
        let mut dup = node;
        dup.id = Uuid::new_v4();
        dup.component.id = dup.id;
        dup.canvas_position += offset;
        dup.selected = false;
        self.nodes.push(dup);
        self.modified = true;
        Some(new_id)
    }

    /// Number of components.
    pub fn component_count(&self) -> usize {
        self.nodes.len()
    }

    /// Iterate all components.
    pub fn iter_components(&self) -> impl Iterator<Item = (usize, &SchematicNode)> {
        self.nodes.iter().enumerate()
    }

    /// Iterate all components mutably.
    pub fn iter_components_mut(&mut self) -> impl Iterator<Item = (usize, &mut SchematicNode)> {
        self.nodes.iter_mut().enumerate()
    }

    // -------------------------------------------------------------------
    // Wire operations
    // -------------------------------------------------------------------

    /// Add a wire between two canvas positions.
    pub fn add_wire(&mut self, start: Vec2, end: Vec2, net_name: &str) -> Uuid {
        let wire = Wire {
            id: Uuid::new_v4(),
            start,
            end,
            net: net_name.to_string(),
        };
        let id = wire.id;
        self.wires.push(wire);
        self.modified = true;
        id
    }

    /// Remove a wire by index.
    pub fn remove_wire(&mut self, idx: usize) -> bool {
        if idx < self.wires.len() {
            self.wires.remove(idx);
            self.modified = true;
            true
        } else {
            false
        }
    }

    /// Number of wires.
    pub fn wire_count(&self) -> usize {
        self.wires.len()
    }

    /// Iterate all wires.
    pub fn iter_wires(&self) -> impl Iterator<Item = (usize, &Wire)> {
        self.wires.iter().enumerate()
    }

    // -------------------------------------------------------------------
    // Net operations
    // -------------------------------------------------------------------

    /// Rebuild nets from wire connectivity.
    /// Lightweight: just groups wires by shared endpoints.
    pub fn rebuild_nets(&mut self) {
        self.nets.clear();
        let mut net_map: std::collections::HashMap<String, Vec<(usize, usize)>> =
            std::collections::HashMap::new();

        // Group wires by net name.
        for wire in &self.wires {
            if !wire.net.is_empty() {
                net_map.entry(wire.net.clone()).or_default();
            }
        }

        // Check which component pins are near wire endpoints.
        let snap_dist_sq = (self.grid_spacing * 0.5) * (self.grid_spacing * 0.5);

        for (ci, node) in self.nodes.iter().enumerate() {
            for (pi, pin) in node.component.pins.iter().enumerate() {
                let pin_world = node.canvas_position + pin.offset;

                for wire in &self.wires {
                    let near_start = (pin_world - wire.start).length_squared() < snap_dist_sq;
                    let near_end = (pin_world - wire.end).length_squared() < snap_dist_sq;

                    if near_start || near_end {
                        net_map
                            .entry(wire.net.clone())
                            .or_default()
                            .push((ci, pi));
                    }
                }
            }
        }

        // Convert to Net structs.
        for (name, pins) in net_map {
            self.nets.push(Net {
                name,
                pins,
                voltage: None,
            });
        }
    }

    /// Get computed nets.
    pub fn nets(&self) -> &[Net] {
        &self.nets
    }

    /// Find which net a pin belongs to.
    pub fn net_for_pin(&self, component_idx: usize, pin_idx: usize) -> Option<&Net> {
        self.nets.iter().find(|net| {
            net.pins.contains(&(component_idx, pin_idx))
        })
    }

    // -------------------------------------------------------------------
    // Selection helpers
    // -------------------------------------------------------------------

    /// Deselect all components.
    pub fn deselect_all(&mut self) {
        for node in &mut self.nodes {
            node.selected = false;
        }
    }

    /// Select a component by index.
    pub fn select_component(&mut self, idx: usize) {
        self.deselect_all();
        if let Some(node) = self.nodes.get_mut(idx) {
            node.selected = true;
        }
    }

    /// Get the index of the selected component (if exactly one).
    pub fn selected_component(&self) -> Option<usize> {
        self.nodes.iter().position(|n| n.selected)
    }

    /// Find the component closest to a canvas position (for click picking).
    pub fn pick_component(&self, canvas_pos: Vec2, radius: f32) -> Option<usize> {
        let radius_sq = radius * radius;
        self.nodes.iter().enumerate()
            .filter(|(_, n)| n.visible)
            .filter(|(_, n)| (n.canvas_position - canvas_pos).length_squared() < radius_sq)
            .min_by(|(_, a), (_, b)| {
                let da = (a.canvas_position - canvas_pos).length_squared();
                let db = (b.canvas_position - canvas_pos).length_squared();
                da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Find the wire closest to a canvas position.
    pub fn pick_wire(&self, canvas_pos: Vec2, threshold: f32) -> Option<usize> {
        let threshold_sq = threshold * threshold;
        self.wires.iter().enumerate()
            .filter_map(|(i, w)| {
                let dist_sq = point_to_segment_dist_sq_2d(canvas_pos, w.start, w.end);
                if dist_sq < threshold_sq {
                    Some((i, dist_sq))
                } else {
                    None
                }
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    // -------------------------------------------------------------------
    // Serialization
    // -------------------------------------------------------------------

    /// Format version marker. Embedded in saved files for compatibility detection.
    pub const FORMAT_VERSION: u32 = 2;

    /// Save to RON file (includes format version marker).
    pub fn save_ron(&self, path: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
        let pretty = ron::ser::PrettyConfig::default();
        let data = ron::ser::to_string_pretty(self, pretty)?;
        // Prepend version comment so loaders can detect format.
        let versioned = format!("// schematic_format_v{}\n{}", Self::FORMAT_VERSION, data);
        std::fs::write(path, versioned)?;
        Ok(())
    }

    /// Load from RON file. Detects legacy format and returns a warning if found.
    /// Returns (graph, Option<warning_message>).
    pub fn load_ron_with_warning(path: &std::path::Path) -> (Self, Option<LegacyWarning>) {
        let raw = match std::fs::read_to_string(path) {
            Ok(data) => data,
            Err(_) => return (Self::default(), None),
        };

        // Strip version comment if present.
        let data = if raw.starts_with("// schematic_format_v") {
            // Modern format - skip first line.
            raw.lines().skip(1).collect::<Vec<_>>().join("\n")
        } else {
            raw.clone()
        };

        // Try parsing as SchematicGraph first (modern format).
        if let Ok(graph) = ron::from_str::<SchematicGraph>(&data) {
            return (graph, None);
        }

        // Try parsing as legacy Schematic format.
        if let Ok(legacy) = ron::from_str::<crate::schematic::Schematic>(&data) {
            let graph = Self::from_legacy_schematic(&legacy);
            let warning = LegacyWarning {
                message_en: format!(
                    "This file uses a legacy schematic format (v1). \
                     It has been converted automatically. \
                     Save to upgrade to the new format (v{}). \
                     If you experience issues, use a previous version \
                     or create a new schematic.",
                    Self::FORMAT_VERSION,
                ),
                message_es: format!(
                    "Este archivo usa un formato de esquematico legacy (v1). \
                     Se ha convertido automaticamente. \
                     Guarda para actualizar al nuevo formato (v{}). \
                     Si tienes problemas, usa una version anterior \
                     o crea un nuevo esquematico.",
                    Self::FORMAT_VERSION,
                ),
                is_legacy: true,
            };
            return (graph, Some(warning));
        }

        // Neither format worked - corrupted file.
        let warning = LegacyWarning {
            message_en: "Could not parse schematic file. \
                         The file may be corrupted or from an incompatible version. \
                         A new empty schematic has been created.".to_string(),
            message_es: "No se pudo leer el archivo de esquematico. \
                         El archivo puede estar corrupto o ser de una version incompatible. \
                         Se ha creado un esquematico vacio.".to_string(),
            is_legacy: false,
        };
        (Self::default(), Some(warning))
    }

    /// Simple load (no warning, just best-effort). For use in non-UI contexts.
    pub fn load_ron(path: &std::path::Path) -> Self {
        let (graph, _warning) = Self::load_ron_with_warning(path);
        graph
    }

    /// Convert to the legacy Schematic format (for backwards compat with existing code).
    pub fn to_legacy_schematic(&self) -> crate::schematic::Schematic {
        let mut s = crate::schematic::Schematic::new(&self.name);
        for node in &self.nodes {
            s.components.push(node.component.clone());
        }
        s.wires = self.wires.clone();
        s
    }

    /// Create from a legacy Schematic (migration path).
    /// The caller should display a warning to the user about the legacy format.
    pub fn from_legacy_schematic(schematic: &crate::schematic::Schematic) -> Self {
        let mut graph = Self::new(&schematic.name);
        for comp in &schematic.components {
            graph.add_component(comp.clone(), comp.position);
        }
        graph.wires = schematic.wires.clone();
        graph.rebuild_nets();
        graph
    }
}

// ---------------------------------------------------------------------------
// Legacy warning
// ---------------------------------------------------------------------------

/// Warning returned when loading a legacy or corrupted schematic file.
/// Both EN and ES messages are provided for the UI to display.
#[derive(Debug, Clone)]
pub struct LegacyWarning {
    /// Warning message in English.
    pub message_en: String,
    /// Warning message in Spanish.
    pub message_es: String,
    /// True if the file was a recognizable legacy format (converted successfully).
    /// False if the file was unreadable/corrupted.
    pub is_legacy: bool,
}

impl Default for SchematicGraph {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

// ---------------------------------------------------------------------------
// Utility
// ---------------------------------------------------------------------------

/// Squared distance from a point to a line segment (2D).
fn point_to_segment_dist_sq_2d(p: Vec2, a: Vec2, b: Vec2) -> f32 {
    let ab = b - a;
    let len_sq = ab.length_squared();
    if len_sq < 0.0001 {
        return (p - a).length_squared();
    }
    let t = ((p - a).dot(ab) / len_sq).clamp(0.0, 1.0);
    let proj = a + ab * t;
    (p - proj).length_squared()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::component::ElectronicComponent;

    fn make_resistor() -> ElectronicComponent {
        ElectronicComponent::resistor("1k")
    }

    #[test]
    fn add_and_count() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_component(make_resistor(), Vec2::new(100.0, 50.0));
        graph.add_component(make_resistor(), Vec2::new(200.0, 50.0));
        assert_eq!(graph.component_count(), 2);
    }

    #[test]
    fn remove_component() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_component(make_resistor(), Vec2::ZERO);
        assert!(graph.remove_component(0));
        assert_eq!(graph.component_count(), 0);
    }

    #[test]
    fn duplicate() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_component(make_resistor(), Vec2::ZERO);
        let dup = graph.duplicate_component(0, Vec2::new(40.0, 20.0));
        assert!(dup.is_some());
        assert_eq!(graph.component_count(), 2);
        // Duplicated component should have different position.
        let pos = graph.get_component(1).unwrap().canvas_position;
        assert!((pos.x - 40.0).abs() < 0.01);
    }

    #[test]
    fn pick_component_nearest() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_component(make_resistor(), Vec2::new(100.0, 100.0));
        graph.add_component(make_resistor(), Vec2::new(300.0, 100.0));

        let picked = graph.pick_component(Vec2::new(110.0, 105.0), 50.0);
        assert_eq!(picked, Some(0));
    }

    #[test]
    fn wire_operations() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_wire(Vec2::ZERO, Vec2::new(100.0, 0.0), "N001");
        assert_eq!(graph.wire_count(), 1);
        assert!(graph.remove_wire(0));
        assert_eq!(graph.wire_count(), 0);
    }

    #[test]
    fn legacy_conversion() {
        let mut graph = SchematicGraph::new("Test");
        graph.add_component(make_resistor(), Vec2::ZERO);
        graph.add_wire(Vec2::ZERO, Vec2::new(50.0, 0.0), "N001");

        let legacy = graph.to_legacy_schematic();
        assert_eq!(legacy.components.len(), 1);
        assert_eq!(legacy.wires.len(), 1);

        let back = SchematicGraph::from_legacy_schematic(&legacy);
        assert_eq!(back.component_count(), 1);
        assert_eq!(back.wire_count(), 1);
    }
}
