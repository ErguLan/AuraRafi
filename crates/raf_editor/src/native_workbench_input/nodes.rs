use super::*;

impl NativeGameWorkbench {
    pub(super) fn handle_nodes_command(&mut self, name: &String, native_input: &NativeUiInputBridge, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        if let Some(slug) = name.strip_prefix("nodes.toggle_category.") {
            self.nodes_host.toggle_category(slug);
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(tool) = name.strip_prefix("nodes.tool.") {
            self.nodes_host.set_active_tool(tool);
            if tool == "fit" {
                self.nodes_fit_view();
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name == "nodes.palette.open" {
            let pointer = native_input.snapshot().pointer_position;
            if let Some(pointer) = pointer {
                if let Some(canvas_point) = self.nodes_canvas_point(pointer) {
                    if let Some(world) = self.nodes_world_point(pointer) {
                        // Snap the spawn point to the dot grid so a
                        // dropped node always lands aligned.
                        let grid = 24.0;
                        let spawn = [
                            (world[0] / grid).round() * grid,
                            (world[1] / grid).round() * grid,
                        ];
                        let _ = canvas_point;
                        self.nodes_host.open_palette_popup(pointer, spawn);
                        // Focus the filter so the node can be found
                        // by typing, like every engine palette.
                        self.host
                            .session_mut()
                            .interaction
                            .focus
                            .request_focus("nodes.palette-popup.search");
                        self.toolbar_revision =
                            self.toolbar_revision.wrapping_add(1).max(1);
                    }
                }
            }
            return true;
        }
        if name == "nodes.palette.close" {
            if self.nodes_host.close_palette_popup() {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if name == "nodes.canvas.pan.start" {
            self.nodes_host.begin_pan();
            return true;
        }
        if name == "nodes.canvas.pan.move" {
            if self.nodes_host.is_panning() {
                self.nodes_pan_by(native_input.snapshot().pointer_delta);
            }
            return true;
        }
        if name == "nodes.canvas.pan.end" {
            self.nodes_host.end_pan();
            return true;
        }
        if name == "nodes.view.fit" {
            self.nodes_fit_view();
            return true;
        }
        if name == "nodes.clear-pending-pin" {
            self.nodes_host.clear_pending_pin();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("nodes.wire.start.") {
            let mut parts = raw.split(':');
            let (Some(raw_node), Some(raw_pin)) = (parts.next(), parts.next()) else {
                return true;
            };
            let is_output = parts.next().is_none_or(|d| d != "in");
            let (Ok(node_id), Ok(pin_id)) = (
                uuid::Uuid::parse_str(raw_node),
                uuid::Uuid::parse_str(raw_pin),
            ) else {
                return true;
            };
            let pointer = native_input
                .snapshot()
                .pointer_position
                .unwrap_or([0.0, 0.0]);
            // Wires are painted in canvas pixels, so the elastic
            // endpoint stays in screen space and lines up with the
            // pin it started from.
            let canvas_point = self.nodes_canvas_point(pointer).unwrap_or(pointer);
            self.nodes_host.begin_wire_drag(
                raf_nodes::NodeId(node_id),
                pin_id,
                canvas_point,
                is_output,
            );
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(_raw) = name.strip_prefix("nodes.wire.move.") {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                let canvas_point = self.nodes_canvas_point(pointer).unwrap_or(pointer);
                self.nodes_host.update_wire_drag(canvas_point);
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            return true;
        }
        if let Some(raw) = name.strip_prefix("nodes.wire.end.") {
            let Some((raw_node, raw_pin)) = raw.split_once(':') else {
                self.nodes_host.clear_wire_drag();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            };
            let (Ok(node_id), Ok(pin_id)) = (
                uuid::Uuid::parse_str(raw_node),
                uuid::Uuid::parse_str(raw_pin),
            ) else {
                self.nodes_host.clear_wire_drag();
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            };
            if let Some(drag) = self.nodes_host.wire_drag() {
                if drag.from_node != raf_nodes::NodeId(node_id)
                    || drag.from_pin != pin_id
                {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "nodes.connect:{}:{}:{}:{}",
                        drag.from_node.0, drag.from_pin, node_id, pin_id,
                    )));
                }
            }
            self.nodes_host.clear_wire_drag();
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("nodes.pin.") {
            let Some((raw_node, raw_pin)) = raw.split_once(':') else {
                return true;
            };
            let (Ok(node_id), Ok(pin_id)) = (
                uuid::Uuid::parse_str(raw_node),
                uuid::Uuid::parse_str(raw_pin),
            ) else {
                return true;
            };
            if let Some(selection) = self
                .nodes_host
                .select_pin(raf_nodes::NodeId(node_id), pin_id)
            {
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "nodes.connect:{}:{}:{}:{}",
                    selection.first.node_id.0,
                    selection.first.pin_id,
                    selection.second.node_id.0,
                    selection.second.pin_id,
                )));
            }
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("nodes.select.") {
            self.nodes_host.clear_pending_pin();
            // Right-click opens the menu for the node it targets;
            // keep it open only when the select is for that node.
            let menu_target = self
                .nodes_host
                .context_menu()
                .map(|menu| menu.node_id.0.to_string());
            if menu_target.as_deref() != Some(raw) {
                self.nodes_host.close_context_menu();
                if self.nodes_host.close_palette_popup() {
                    self.host.session_mut().interaction.focus.clear_focus();
                }
            }
            intents.push(NativeWorkbenchIntent::Command(name.clone()));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name.starts_with("nodes.delete.")
            || name.starts_with("nodes.disconnect_all.")
            || name.starts_with("nodes.duplicate.")
        {
            // Executing a menu action closes the menu.
            self.nodes_host.close_context_menu();
            self.nodes_host.clear_pending_pin();
            intents.push(NativeWorkbenchIntent::Command(name.clone()));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if let Some(raw) = name.strip_prefix("nodes.property.commit:") {
            let Some((raw_node, key)) = raw.split_once(':') else {
                return true;
            };
            let Ok(node_id) = uuid::Uuid::parse_str(raw_node) else {
                return true;
            };
            let value_key = format!("nodes.property.{raw_node}.{key}");
            let value = self
                .host
                .session()
                .interaction
                .controls
                .text(&value_key)
                .to_string();
            intents.push(NativeWorkbenchIntent::Command(format!(
                "nodes.property.set:{node_id}:{key}:{value}"
            )));
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return true;
        }
        if name.starts_with("nodes.add.at:") {
            // Picking a row drops the node and retires the popup.
            if self.nodes_host.close_palette_popup() {
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            }
            intents.push(NativeWorkbenchIntent::Command(name.clone()));
            return true;
        }
        if let Some(raw_id) = name.strip_prefix("nodes.drag.start.") {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                if let Some(world) = self.nodes_world_point(pointer) {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "nodes.drag.start:{raw_id}:{:.3}:{:.3}",
                        world[0], world[1]
                    )));
                }
            }
            return true;
        }
        if let Some(raw_id) = name.strip_prefix("nodes.drag.move.") {
            if let Some(pointer) = native_input.snapshot().pointer_position {
                if let Some(world) = self.nodes_world_point(pointer) {
                    intents.push(NativeWorkbenchIntent::Command(format!(
                        "nodes.drag.move:{raw_id}:{:.3}:{:.3}",
                        world[0], world[1]
                    )));
                }
            }
            return true;
        }
        if name.starts_with("nodes.drag.end.") {
            intents.push(NativeWorkbenchIntent::Command("nodes.drag.end".to_string()));
            return true;
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_nodes_zoom_command(&mut self, name: &String, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        match name.as_str() {
            "nodes.zoom.in" => {
                let size = self.nodes_canvas_size();
                self.nodes_zoom_at([size[0] * 0.5, size[1] * 0.5], 1.12);
                return true;
            }
            "nodes.zoom.out" => {
                let size = self.nodes_canvas_size();
                self.nodes_zoom_at([size[0] * 0.5, size[1] * 0.5], 1.0 / 1.12);
                return true;
            }
            "nodes.zoom.reset" => {
                self.nodes_zoom = 1.0;
                self.nodes_pan = [0.0, 0.0];
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
            "nodes.undo" | "nodes.redo" | "nodes.clear-selection" => {
                if self.nodes_host.close_context_menu() {
                    self.toolbar_revision =
                        self.toolbar_revision.wrapping_add(1).max(1);
                }
                if self.nodes_host.close_palette_popup() {
                    self.toolbar_revision =
                        self.toolbar_revision.wrapping_add(1).max(1);
                }
                intents.push(NativeWorkbenchIntent::Command(name.clone()));
                return true;
            }
            _ => {}
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_nodes_open_menu(&mut self, id: &String, native_input: &NativeUiInputBridge, intents: &mut Vec<NativeWorkbenchIntent>) -> bool {
        if let Some(raw_id) = id.strip_prefix("nodes.context.") {
            if let Ok(parsed) = uuid::Uuid::parse_str(raw_id) {
                self.open_menu = None;
                self.open_submenu = None;
                // One pointer gesture opens one menu: retire the
                // other retained context surfaces.
                self.hierarchy_empty_menu_open = false;
                self.hierarchy_primitive_menu_open = false;
                self.hierarchy_menu_target = None;
                self.hierarchy_menu_position = None;
                self.nodes_host.open_context_menu(
                    raf_nodes::NodeId(parsed),
                    native_input
                        .snapshot()
                        .pointer_position
                        .unwrap_or([0.0, 0.0]),
                );
                intents.push(NativeWorkbenchIntent::Command(format!(
                    "nodes.select.{raw_id}"
                )));
                self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
                return true;
            }
        }
        false
    }
}

impl NativeGameWorkbench {
    pub(super) fn handle_nodes_pointer_and_keyboard(&mut self, native_input: &NativeUiInputBridge, intents: &mut Vec<NativeWorkbenchIntent>) {
        let snapshot = native_input.snapshot();
        let nodes_text_input_focused = self
            .host
            .session()
            .interaction
            .focus
            .focused
            .as_deref()
            .is_some_and(|id| {
                id == "nodes.search"
                    || id.starts_with("nodes.property.control.")
                    || id.starts_with("nodes.palette-popup.")
            });
        let nodes_tab_active = self.bottom_dock.has_active_tab("nodes");
        // The Nodes dock stays reachable from an Electronics project, so its
        // single-key and undo bindings used to fire from anywhere: `F` over the
        // schematic both framed the schematic and re-framed a graph the user was
        // not looking at, and `Ctrl+Z` undid the node graph and the documents in
        // the same keypress. A keyboard binding belongs to the surface the user
        // is actually working on, so outside a Game project it only applies while
        // the pointer is over the Nodes canvas. The wheel and middle-drag arms
        // were already spatially bounded; they share the gate so the whole block
        // has one rule instead of three.
        let pointer_in_nodes_canvas = snapshot
            .pointer_position
            .is_some_and(|pointer| self.nodes_canvas_point(pointer).is_some());
        let nodes_surface_active =
            nodes_tab_active && (self.project_type == ProjectType::Game || pointer_in_nodes_canvas);
        if nodes_surface_active
            && snapshot.modifiers.command_modifier()
            && !nodes_text_input_focused
        {
            let nodes_command = if snapshot.key_pressed(raf_core::InputKey::Z) {
                Some("nodes.undo")
            } else if snapshot.key_pressed(raf_core::InputKey::Y) {
                Some("nodes.redo")
            } else {
                None
            };
            if let Some(command) = nodes_command {
                intents.push(NativeWorkbenchIntent::Command(command.to_string()));
            }
        }
        // Nodes viewport navigation: wheel zooms around the pointer, the middle
        // button pans, and `F` frames the graph. Text fields keep the keyboard.
        if nodes_surface_active && !nodes_text_input_focused {
            if snapshot.key_pressed(raf_core::InputKey::F) {
                self.nodes_fit_view();
            }
            if snapshot.scroll_delta[1].abs() > f32::EPSILON {
                if let Some(pointer) = snapshot.pointer_position {
                    if let Some(canvas_point) = self.nodes_canvas_point(pointer) {
                        // Scrolling up zooms in, matching every node editor.
                        let factor = (1.0 - snapshot.scroll_delta[1] * 0.12).clamp(0.5, 2.0);
                        self.nodes_zoom_at(canvas_point, factor);
                    }
                }
            }
            if snapshot.button_down(raf_core::PointerButton::Middle) {
                self.nodes_pan_by(snapshot.pointer_delta);
            }
        }
    }
}

impl NativeGameWorkbench {
    // ------------------------------------------------------------------
    // NODES CANVAS GEOMETRY
    //
    // The Nodes canvas is a fixed viewport, so every pointer interaction needs
    // the window-to-canvas mapping: window -> dock content -> graph panel ->
    // canvas, and then world units through zoom and pan.
    // ------------------------------------------------------------------

    /// Origin of the Nodes canvas in window coordinates, or `None` when the
    /// Nodes tab is not the active dock tab.
    fn nodes_canvas_origin(&self) -> Option<[f32; 2]> {
        self.nodes_canvas_frame().map(|(origin, _)| origin)
    }

    /// Origin and size of the Nodes canvas in window coordinates. The canvas
    /// only occupies its own dock column, so the size cannot be derived from
    /// the whole bottom dock.
    fn nodes_canvas_frame(&self) -> Option<([f32; 2], [f32; 2])> {
        let layout = self.last_layout?;
        if !self.bottom_dock.has_active_tab("nodes") {
            return None;
        }
        let group_rect = self
            .dock_group_rects(layout)
            .into_iter()
            .find(|(group_id, _)| {
                self.bottom_dock
                    .groups()
                    .iter()
                    .any(|group| &group.id == group_id && group.active_tab == "nodes")
            })
            .map(|(_, rect)| rect)?;
        let tab_height = group_rect.height.min(34.0).max(1.0);
        Some((
            [
                group_rect.x,
                group_rect.y + tab_height + crate::panels::nodes_surface::NODES_TOOLBAR_HEIGHT,
            ],
            [
                group_rect.width.max(160.0),
                (group_rect.height
                    - tab_height
                    - crate::panels::nodes_surface::NODES_TOOLBAR_HEIGHT)
                    .max(120.0),
            ],
        ))
    }

    /// Canvas size in pixels, used to clamp panning and to frame the graph.
    fn nodes_canvas_size(&self) -> [f32; 2] {
        self.nodes_canvas_frame()
            .map_or([860.0, 560.0], |(_, size)| size)
    }

    /// Pointer position in canvas pixels, or `None` when it is outside.
    fn nodes_canvas_point(&self, pointer: [f32; 2]) -> Option<[f32; 2]> {
        let origin = self.nodes_canvas_origin()?;
        let point = [pointer[0] - origin[0], pointer[1] - origin[1]];
        let size = self.nodes_canvas_size();
        if point[0] < 0.0 || point[1] < 0.0 || point[0] > size[0] || point[1] > size[1] {
            return None;
        }
        Some(point)
    }

    /// Pointer position in graph world units, which is what the runtime stores
    /// for node positions and wire endpoints.
    fn nodes_world_point(&self, pointer: [f32; 2]) -> Option<[f32; 2]> {
        let point = self.nodes_canvas_point(pointer)?;
        let zoom = self.nodes_zoom.max(0.2);
        Some([
            (point[0] + self.nodes_pan[0]) / zoom,
            (point[1] + self.nodes_pan[1]) / zoom,
        ])
    }

    /// Zooms around a fixed canvas point so the world position under the
    /// pointer does not move.
    fn nodes_zoom_at(&mut self, canvas_point: [f32; 2], factor: f32) {
        let previous = self.nodes_zoom;
        let next = (previous * factor).clamp(0.4, 2.0);
        if (next - previous).abs() <= f32::EPSILON {
            return;
        }
        let world = [
            (canvas_point[0] + self.nodes_pan[0]) / previous,
            (canvas_point[1] + self.nodes_pan[1]) / previous,
        ];
        self.nodes_pan = [
            canvas_point[0] - world[0] * next,
            canvas_point[1] - world[1] * next,
        ];
        self.nodes_zoom = next;
        self.nodes_host.end_pan();
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    fn nodes_pan_by(&mut self, delta: [f32; 2]) {
        let size = self.nodes_canvas_size();
        self.nodes_pan[0] = (self.nodes_pan[0] - delta[0]).clamp(-size[0] * 2.0, size[0] * 2.0);
        self.nodes_pan[1] = (self.nodes_pan[1] - delta[1]).clamp(-size[1] * 2.0, size[1] * 2.0);
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }

    /// Centers and scales the viewport so the whole graph is visible. This is
    /// what the Fit tool and the `F` key do. Bounds are cached by `sync` so
    /// input never has to walk the graph.
    fn nodes_fit_view(&mut self) {
        let [min_x, min_y, width, height] = self.nodes_graph_bounds;
        if width <= 0.0 || height <= 0.0 {
            self.nodes_pan = [0.0, 0.0];
            self.nodes_zoom = 1.0;
            self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
            return;
        }
        let size = self.nodes_canvas_size();
        let zoom = ((size[0] - 48.0) / width)
            .min((size[1] - 48.0) / height)
            .clamp(0.4, 1.6);
        self.nodes_zoom = zoom;
        self.nodes_pan = [
            (size[0] - width * zoom) * 0.5 - min_x * zoom,
            (size[1] - height * zoom) * 0.5 - min_y * zoom,
        ];
        self.toolbar_revision = self.toolbar_revision.wrapping_add(1).max(1);
    }
}
