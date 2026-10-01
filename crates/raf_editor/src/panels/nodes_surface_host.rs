//! Transient host state for the retained Nodes surface.

use raf_nodes::NodeId;
use raf_ui::{UiMotionSpec, UiTween};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PendingPin {
    pub node_id: NodeId,
    pub pin_id: Uuid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PinSelection {
    pub first: PendingPin,
    pub second: PendingPin,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodesWireDrag {
    pub from_node: NodeId,
    pub from_pin: Uuid,
    pub pointer_canvas: [f32; 2],
    pub is_output: bool,
}

/// Pointer-anchored context menu for a single node card. The position is
/// stored in window coordinates so the workbench can append it as a
/// window-level overlay outside the clipped dock content.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodesContextMenu {
    pub node_id: NodeId,
    pub position: [f32; 2],
}

/// Quick-add palette anchored at the pointer, in window coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodesPalettePopup {
    pub position: [f32; 2],
    /// Canvas point the new node is dropped on, already converted to world
    /// units so the runtime never has to know about pan or zoom.
    pub spawn: [f32; 2],
}

#[derive(Debug, Clone)]
pub struct NodesSurfaceHost {
    query: String,
    pending_pin: Option<PendingPin>,
    wire_drag: Option<NodesWireDrag>,
    collapsed_categories: std::collections::HashSet<String>,
    active_flow: String,
    active_tool: String,
    property_drafts: HashMap<String, String>,
    context_menu: Option<NodesContextMenu>,
    palette_popup: Option<NodesPalettePopup>,
    palette_query: String,
    panning: bool,
    selection_glow: UiTween,
    menu_motion: UiTween,
    last_selected: Option<NodeId>,
}

impl Default for NodesSurfaceHost {
    fn default() -> Self {
        Self {
            query: String::new(),
            pending_pin: None,
            wire_drag: None,
            collapsed_categories: std::collections::HashSet::new(),
            active_flow: "Main Event Graph".to_string(),
            active_tool: "select".to_string(),
            property_drafts: HashMap::new(),
            context_menu: None,
            palette_popup: None,
            palette_query: String::new(),
            panning: false,
            selection_glow: UiTween::new(0.0, UiMotionSpec::dock()),
            menu_motion: UiTween::new(0.0, UiMotionSpec::tooltip()),
            last_selected: None,
        }
    }
}

impl NodesSurfaceHost {
    pub fn query(&self) -> &str {
        &self.query
    }

    pub fn set_query(&mut self, query: impl Into<String>) {
        self.query = query.into();
    }

    pub fn set_property_draft(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.property_drafts.insert(key.into(), value.into());
    }

    pub fn property_draft(&self, key: &str) -> Option<&str> {
        self.property_drafts.get(key).map(String::as_str)
    }

    pub fn pending_pin(&self) -> Option<PendingPin> {
        self.pending_pin
    }

    pub fn set_pending_pin(&mut self, pin: PendingPin) {
        self.pending_pin = Some(pin);
    }

    pub fn wire_drag(&self) -> Option<NodesWireDrag> {
        self.wire_drag
    }

    pub fn begin_wire_drag(&mut self, from_node: NodeId, from_pin: Uuid, pointer: [f32; 2], is_output: bool) {
        self.pending_pin = Some(PendingPin { node_id: from_node, pin_id: from_pin });
        self.wire_drag = Some(NodesWireDrag {
            from_node,
            from_pin,
            pointer_canvas: pointer,
            is_output,
        });
    }

    pub fn update_wire_drag(&mut self, pointer: [f32; 2]) {
        if let Some(drag) = self.wire_drag.as_mut() {
            drag.pointer_canvas = pointer;
        }
    }

    pub fn clear_wire_drag(&mut self) {
        self.wire_drag = None;
    }

    pub fn active_flow(&self) -> &str {
        &self.active_flow
    }

    pub fn set_active_flow(&mut self, flow: impl Into<String>) {
        self.active_flow = flow.into();
    }

    pub fn active_tool(&self) -> &str {
        &self.active_tool
    }

    pub fn set_active_tool(&mut self, tool: impl Into<String>) {
        self.active_tool = tool.into();
    }

    pub fn toggle_category(&mut self, slug: &str) {
        if self.collapsed_categories.contains(slug) {
            self.collapsed_categories.remove(slug);
        } else {
            self.collapsed_categories.insert(slug.to_string());
        }
    }

    pub fn is_category_collapsed(&self, slug: &str) -> bool {
        self.collapsed_categories.contains(slug)
    }

    pub fn select_pin(&mut self, node_id: NodeId, pin_id: Uuid) -> Option<PinSelection> {
        let next = PendingPin { node_id, pin_id };
        match self.pending_pin.replace(next) {
            Some(first) if first != next => {
                self.pending_pin = None;
                self.wire_drag = None;
                Some(PinSelection {
                    first,
                    second: next,
                })
            }
            Some(_) => {
                self.pending_pin = None;
                self.wire_drag = None;
                None
            }
            None => None,
        }
    }

    pub fn clear_pending_pin(&mut self) {
        self.pending_pin = None;
        self.wire_drag = None;
    }

    pub fn open_context_menu(&mut self, node_id: NodeId, position: [f32; 2]) {
        self.palette_popup = None;
        self.context_menu = Some(NodesContextMenu { node_id, position });
        self.menu_motion.set_immediate(0.0);
        self.menu_motion.set_target(1.0);
    }

    /// Opens the quick-add palette at a window position, remembering the world
    /// point where a picked node must land.
    pub fn open_palette_popup(&mut self, position: [f32; 2], spawn: [f32; 2]) {
        self.context_menu = None;
        self.palette_popup = Some(NodesPalettePopup { position, spawn });
        self.palette_query.clear();
        self.menu_motion.set_immediate(0.0);
        self.menu_motion.set_target(1.0);
    }

    pub fn close_palette_popup(&mut self) -> bool {
        let was_open = self.palette_popup.take().is_some();
        if was_open {
            self.palette_query.clear();
            self.menu_motion.set_immediate(0.0);
        }
        was_open
    }

    pub fn palette_popup(&self) -> Option<NodesPalettePopup> {
        self.palette_popup
    }

    pub fn palette_query(&self) -> &str {
        &self.palette_query
    }

    pub fn set_palette_query(&mut self, query: impl Into<String>) {
        self.palette_query = query.into();
    }

    pub fn begin_pan(&mut self) {
        self.panning = true;
    }

    pub fn is_panning(&self) -> bool {
        self.panning
    }

    pub fn end_pan(&mut self) {
        self.panning = false;
    }

    /// True while any Nodes overlay owns the pointer, so outside interaction
    /// must not dismiss the surface under it.
    pub fn has_open_overlay(&self) -> bool {
        self.context_menu.is_some() || self.palette_popup.is_some()
    }

    pub fn close_context_menu(&mut self) -> bool {
        let was_open = self.context_menu.take().is_some();
        if was_open {
            self.menu_motion.set_immediate(0.0);
        }
        was_open
    }

    pub fn context_menu(&self) -> Option<NodesContextMenu> {
        self.context_menu
    }

    pub fn has_open_menu(&self) -> bool {
        self.context_menu.is_some()
    }

    /// Replays the selection reveal each time the selected graph node changes
    /// so the ring animates instead of popping in on a rebuilt surface.
    pub fn note_selection(&mut self, selected: Option<NodeId>) {
        if self.last_selected == selected {
            return;
        }
        self.last_selected = selected;
        match selected {
            Some(_) => {
                self.selection_glow.set_immediate(0.0);
                self.selection_glow.set_target(1.0);
            }
            None => {
                self.selection_glow.set_target(0.0);
            }
        }
    }

    pub fn selection_glow(&self) -> f32 {
        self.selection_glow.value()
    }

    pub fn menu_motion(&self) -> f32 {
        self.menu_motion.value()
    }

    pub fn tick_motion(&mut self, delta_seconds: f32, reduced_motion: bool) {
        self.selection_glow.advance(delta_seconds, reduced_motion);
        self.menu_motion.advance(delta_seconds, reduced_motion);
    }

    /// True while a Nodes reveal/menu tween still needs repaint frames.
    pub fn is_animating(&self) -> bool {
        !self.selection_glow.is_settled() || !self.menu_motion.is_settled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_menu_opens_replaces_and_closes() {
        let mut host = NodesSurfaceHost::default();
        let first = NodeId::new();
        let second = NodeId::new();

        assert!(!host.has_open_menu());
        host.open_context_menu(first, [120.0, 240.0]);
        let open = host.context_menu().expect("menu must be open");
        assert_eq!(open.node_id, first);
        assert_eq!(open.position, [120.0, 240.0]);

        host.open_context_menu(second, [10.0, 20.0]);
        assert_eq!(
            host.context_menu().map(|menu| menu.node_id),
            Some(second),
            "a second right click replaces the menu target"
        );

        assert!(host.close_context_menu());
        assert!(!host.has_open_menu());
        assert!(!host.close_context_menu());
    }

    #[test]
    fn selection_changes_drive_a_reveal_tween_that_settles() {
        let mut host = NodesSurfaceHost::default();
        let node = NodeId::new();
        host.note_selection(Some(node));
        assert_eq!(host.selection_glow(), 0.0, "reveal must start below target");
        assert!(host.is_animating());

        host.tick_motion(1.0, false);
        assert_eq!(host.selection_glow(), 1.0);
        assert!(!host.is_animating());

        host.note_selection(None);
        assert!(host.is_animating());
        host.tick_motion(1.0, false);
        assert_eq!(host.selection_glow(), 0.0);
        assert!(!host.is_animating());
    }

    #[test]
    fn repeated_selection_of_the_same_node_does_not_restart_motion() {
        let mut host = NodesSurfaceHost::default();
        let node = NodeId::new();
        host.note_selection(Some(node));
        host.tick_motion(1.0, false);
        host.note_selection(Some(node));
        assert!(!host.is_animating());
    }

    #[test]
    fn quick_add_palette_and_context_menu_are_mutually_exclusive() {
        let mut host = NodesSurfaceHost::default();
        let node = NodeId::new();

        host.open_context_menu(node, [10.0, 10.0]);
        assert!(host.has_open_overlay());

        host.open_palette_popup([120.0, 140.0], [48.0, 72.0]);
        assert!(host.context_menu().is_none(), "one gesture opens one overlay");
        let popup = host.palette_popup().expect("palette is open");
        assert_eq!(popup.position, [120.0, 140.0]);
        assert_eq!(popup.spawn, [48.0, 72.0]);
        assert!(host.palette_query().is_empty(), "a fresh popup starts empty");

        host.set_palette_query("print");
        assert!(host.close_palette_popup());
        assert!(host.palette_query().is_empty(), "closing clears the filter");
        assert!(!host.has_open_overlay());
    }

    #[test]
    fn pan_gesture_is_tracked_until_it_ends() {
        let mut host = NodesSurfaceHost::default();
        assert!(!host.is_panning());
        host.begin_pan();
        assert!(host.is_panning());
        host.end_pan();
        assert!(!host.is_panning());
    }
}
