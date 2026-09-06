//! Native editor orchestration without a widget-toolkit dependency.
//!
//! Winit feeds an `InputSnapshot`; RafUI surfaces and the active canvas share
//! this router, while ApiGraphicBasic owns frame pacing and composition.

use std::sync::Arc;
use std::time::Instant;

use raf_core::config::EngineSettings;
use raf_core::project::ProjectType;
use raf_core::scene::SceneGraph;
use raf_core::{InputRouter, InputSnapshot, Revision, UndoToken};
use raf_render::api_graphic_basic::{
    DynamicResolutionController, EditorCanvasLayer, FrameInvalidation, FramePacingProfile,
    FramePermit, SceneFrameCapture,
};
use raf_render::bridge::{GraphicsSurfaceKind, RenderRuntime, ViewportInputRect};

use crate::editor_command_registry::{EditorCommandAvailability, EditorCommandRegistry};
use crate::editor_layout::{
    EditorFrameLayout, EditorLayoutRequest, EDITOR_DOCK_MAX_HEIGHT, EDITOR_DOCK_MIN_HEIGHT,
};
use crate::panels::viewport_controller::{NativeGameViewportController, NativeViewportUpdate};
use crate::scene_history::SceneHistory;

fn canvas_requires_render(reasons: FrameInvalidation) -> bool {
    reasons.intersects(
        FrameInvalidation::WINDOW
            | FrameInvalidation::DOCUMENT
            | FrameInvalidation::CAMERA
            | FrameInvalidation::SIMULATION
            | FrameInvalidation::ASSET_UPLOAD
            | FrameInvalidation::EXPLICIT,
    )
}

pub struct NativeEditorRuntime {
    input_router: InputRouter,
    game_viewport: NativeGameViewportController,
    graphics: RenderRuntime,
    layout_request: EditorLayoutRequest,
    layout: EditorFrameLayout,
    scale_factor: f32,
    target_size: [u32; 2],
    dynamic_resolution: DynamicResolutionController,
    project_type: ProjectType,
    active_frame: Option<FramePermit>,
    cached_game_canvas: Option<CachedGameCanvas>,
    canvas_renders: u64,
    canvas_cache_hits: u64,
    frame_started_at: Option<Instant>,
    last_frame_cpu_ms: f32,
    command_registry: EditorCommandRegistry,
    history: SceneHistory,
    attached_undo: Option<AttachedSceneUndo>,
    clipboard: Vec<raf_core::scene::SceneNodeId>,
    node_graph: raf_nodes::NodeGraph,
    selected_graph_node: Option<raf_nodes::NodeId>,
    node_drag: Option<(raf_nodes::NodeId, [f32; 2], [f32; 2])>,
}

#[derive(Clone)]
struct CachedGameCanvas {
    device_generation: u64,
    layer: EditorCanvasLayer,
}

struct AttachedSceneUndo {
    token: UndoToken,
    revision_after: Revision,
    before: SceneGraph,
    after_fingerprint: u64,
}

impl NativeEditorRuntime {
    pub fn new(layout_request: EditorLayoutRequest) -> Self {
        let layout = EditorFrameLayout::compute(layout_request);
        Self {
            input_router: InputRouter::default(),
            game_viewport: NativeGameViewportController::default(),
            graphics: RenderRuntime::default(),
            layout_request,
            layout,
            scale_factor: 1.0,
            target_size: [
                layout_request.logical_size[0].round().max(1.0) as u32,
                layout_request.logical_size[1].round().max(1.0) as u32,
            ],
            dynamic_resolution: DynamicResolutionController::default(),
            project_type: ProjectType::Game,
            active_frame: None,
            cached_game_canvas: None,
            canvas_renders: 0,
            canvas_cache_hits: 0,
            frame_started_at: None,
            last_frame_cpu_ms: 0.0,
            command_registry: EditorCommandRegistry::default(),
            history: SceneHistory::default(),
            attached_undo: None,
            clipboard: Vec::new(),
            node_graph: raf_nodes::NodeGraph::new("Main"),
            selected_graph_node: None,
            node_drag: None,
        }
    }

    pub fn node_graph(&self) -> &raf_nodes::NodeGraph {
        &self.node_graph
    }

    pub fn selected_graph_node(&self) -> Option<raf_nodes::NodeId> {
        self.selected_graph_node
    }

    pub fn set_node_graph(&mut self, graph: raf_nodes::NodeGraph) {
        self.node_graph = graph;
        self.selected_graph_node = None;
        self.node_drag = None;
        self.request_document_frame();
    }

    pub fn select_graph_node(&mut self, id: raf_nodes::NodeId) {
        if self.node_graph.nodes.iter().any(|node| node.id == id) {
            self.selected_graph_node = Some(id);
            self.request_ui_frame();
        }
    }

    pub fn add_graph_node(&mut self, node: raf_nodes::Node) {
        let id = self.node_graph.add_node(node);
        self.selected_graph_node = Some(id);
        self.request_document_frame();
    }

    pub fn delete_graph_node(&mut self, id: raf_nodes::NodeId) {
        self.node_graph.remove_node(id);
        self.node_drag = None;
        if self.selected_graph_node == Some(id) {
            self.selected_graph_node = None;
        }
        self.request_document_frame();
    }

    pub fn reset_graph(&mut self) {
        self.node_graph = raf_nodes::NodeGraph::new("Main");
        self.selected_graph_node = None;
        self.node_drag = None;
        self.request_document_frame();
    }

    pub fn begin_graph_node_drag(&mut self, id: raf_nodes::NodeId, pointer: [f32; 2]) {
        let Some(node) = self.node_graph.nodes.iter().find(|node| node.id == id) else {
            return;
        };
        self.selected_graph_node = Some(id);
        self.node_drag = Some((id, pointer, node.position));
        self.request_ui_frame();
    }

    pub fn move_graph_node_drag(&mut self, id: raf_nodes::NodeId, pointer: [f32; 2]) {
        let Some((drag_id, origin_pointer, origin_position)) = self.node_drag else {
            return;
        };
        if drag_id != id {
            return;
        }
        if let Some(node) = self.node_graph.nodes.iter_mut().find(|node| node.id == id) {
            node.position = [
                origin_position[0] + pointer[0] - origin_pointer[0],
                origin_position[1] + pointer[1] - origin_pointer[1],
            ];
            self.request_document_frame();
        }
    }

    pub fn end_graph_node_drag(&mut self) {
        self.node_drag = None;
        self.request_ui_frame();
    }

    pub fn input_router(&self) -> &InputRouter {
        &self.input_router
    }

    pub fn input_router_mut(&mut self) -> &mut InputRouter {
        &mut self.input_router
    }

    pub fn game_viewport(&self) -> &NativeGameViewportController {
        &self.game_viewport
    }

    pub fn game_viewport_mut(&mut self) -> &mut NativeGameViewportController {
        &mut self.game_viewport
    }

    pub fn history(&self) -> &SceneHistory {
        &self.history
    }

    /// Record the snapshot owned by an attached Game command after the
    /// command gateway has confirmed that the scene changed. The normal
    /// editor history also receives the snapshot, so Ctrl+Z remains useful;
    /// the extra token is only for the exact external transaction.
    pub fn record_attached_scene_change(
        &mut self,
        before: SceneGraph,
        scene: &SceneGraph,
        revision_after: Revision,
    ) -> Option<UndoToken> {
        if crate::scene_history::scene_fingerprint(&before)
            == crate::scene_history::scene_fingerprint(scene)
        {
            return None;
        }
        self.history.record_if_changed(before.clone(), scene);
        let token = UndoToken::new();
        self.attached_undo = Some(AttachedSceneUndo {
            token,
            revision_after,
            before,
            after_fingerprint: crate::scene_history::scene_fingerprint(scene),
        });
        Some(token)
    }

    pub fn can_undo_attached_scene(
        &self,
        scene: &SceneGraph,
        token: UndoToken,
        current_revision: Revision,
    ) -> bool {
        self.attached_undo.as_ref().is_some_and(|undo| {
            undo.token == token
                && undo.revision_after == current_revision
                && undo.after_fingerprint == crate::scene_history::scene_fingerprint(scene)
        })
    }

    pub fn undo_attached_scene(
        &mut self,
        scene: &mut SceneGraph,
        token: UndoToken,
        current_revision: Revision,
    ) -> bool {
        let Some(undo) = self.attached_undo.take() else {
            return false;
        };
        let valid = undo.token == token
            && undo.revision_after == current_revision
            && undo.after_fingerprint == crate::scene_history::scene_fingerprint(scene);
        if !valid {
            self.attached_undo = Some(undo);
            return false;
        }
        *scene = undo.before;
        self.retain_valid_selection(scene);
        self.request_document_frame();
        true
    }

    pub fn select_node(&mut self, scene: &SceneGraph, id: raf_core::scene::SceneNodeId) {
        if scene.is_valid_node(id) {
            self.game_viewport.selected = vec![id];
            self.request_overlay_frame();
        }
    }

    pub fn select_node_with_modifier(
        &mut self,
        scene: &SceneGraph,
        id: raf_core::scene::SceneNodeId,
        additive: bool,
    ) {
        if !scene.is_valid_node(id) {
            return;
        }
        if additive {
            if let Some(index) = self
                .game_viewport
                .selected
                .iter()
                .position(|selected| *selected == id)
            {
                self.game_viewport.selected.remove(index);
            } else {
                self.game_viewport.selected.push(id);
            }
        } else {
            self.game_viewport.selected = vec![id];
        }
        self.request_overlay_frame();
    }

    pub fn select_node_range(
        &mut self,
        scene: &SceneGraph,
        ordered: &[raf_core::scene::SceneNodeId],
        anchor: raf_core::scene::SceneNodeId,
        target: raf_core::scene::SceneNodeId,
    ) {
        if !scene.is_valid_node(anchor) || !scene.is_valid_node(target) {
            return;
        }
        let Some(anchor_index) = ordered.iter().position(|id| *id == anchor) else {
            return;
        };
        let Some(target_index) = ordered.iter().position(|id| *id == target) else {
            return;
        };
        let start = anchor_index.min(target_index);
        let end = anchor_index.max(target_index);
        self.game_viewport.selected = ordered[start..=end]
            .iter()
            .copied()
            .filter(|id| scene.is_valid_node(*id))
            .collect();
        self.request_overlay_frame();
    }

    pub fn focus_selection(&mut self, scene: &SceneGraph) {
        self.game_viewport.focus_selection(scene);
        self.graphics.request_frame(FrameInvalidation::CAMERA);
    }

    pub fn create_primitive(
        &mut self,
        scene: &mut SceneGraph,
        primitive: raf_core::scene::Primitive,
    ) {
        let before = scene.clone();
        let id = scene.add_root_with_primitive(
            &format!("{} {}", primitive.label(), scene.len() + 1),
            primitive,
        );
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn create_primitive_under(
        &mut self,
        scene: &mut SceneGraph,
        parent: raf_core::scene::SceneNodeId,
        primitive: raf_core::scene::Primitive,
    ) {
        if !scene.is_valid_node(parent) {
            return;
        }
        let before = scene.clone();
        let child_count = scene
            .get(parent)
            .map(|node| node.children.len())
            .unwrap_or_default();
        let id = scene.add_child_with_primitive(
            parent,
            &format!("{} {}", primitive.label(), child_count + 1),
            primitive,
        );
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn create_entity(&mut self, scene: &mut SceneGraph) {
        let before = scene.clone();
        let id = scene.add_root(&format!("Entity {}", scene.len() + 1));
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn create_entity_under(
        &mut self,
        scene: &mut SceneGraph,
        parent: raf_core::scene::SceneNodeId,
    ) {
        if !scene.is_valid_node(parent) {
            return;
        }
        let before = scene.clone();
        let child_count = scene
            .get(parent)
            .map(|node| node.children.len())
            .unwrap_or_default();
        let id = scene.add_child(parent, &format!("Entity {}", child_count + 1));
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn create_folder(&mut self, scene: &mut SceneGraph) {
        let before = scene.clone();
        let id = scene.add_root_folder(&format!("Folder {}", scene.len() + 1));
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn create_folder_under(
        &mut self,
        scene: &mut SceneGraph,
        parent: raf_core::scene::SceneNodeId,
    ) {
        if !scene.is_valid_node(parent) {
            return;
        }
        let before = scene.clone();
        let child_count = scene
            .get(parent)
            .map(|node| node.children.len())
            .unwrap_or_default();
        let id = scene.add_child_folder(parent, &format!("Folder {}", child_count + 1));
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = vec![id];
        self.request_document_frame();
    }

    pub fn mutate_scene<F>(&mut self, scene: &mut SceneGraph, mutate: F) -> bool
    where
        F: FnOnce(&mut SceneGraph),
    {
        let before = scene.clone();
        mutate(scene);
        let changed = self.history.record_if_changed(before, scene);
        if changed {
            self.request_document_frame();
        }
        changed
    }

    pub fn duplicate_selected(&mut self, scene: &mut SceneGraph) {
        self.duplicate_selection(scene);
    }

    pub fn delete_selected(&mut self, scene: &mut SceneGraph) {
        self.delete_selection(scene);
    }

    pub fn undo(&mut self, scene: &mut SceneGraph) {
        if self.history.undo(scene) {
            self.retain_valid_selection(scene);
            self.request_document_frame();
        }
    }

    pub fn redo(&mut self, scene: &mut SceneGraph) {
        if self.history.redo(scene) {
            self.retain_valid_selection(scene);
            self.request_document_frame();
        }
    }

    pub fn copy_selected(&mut self) {
        self.clipboard = self.game_viewport.selected.clone();
    }

    pub fn copy_node(&mut self, id: raf_core::scene::SceneNodeId) {
        self.clipboard = vec![id];
    }

    pub fn has_clipboard(&self) -> bool {
        !self.clipboard.is_empty()
    }

    pub fn paste_selected(&mut self, scene: &mut SceneGraph) {
        self.paste_clipboard(scene);
    }

    pub fn paste_into(
        &mut self,
        scene: &mut SceneGraph,
        parent: Option<raf_core::scene::SceneNodeId>,
    ) {
        let before = scene.clone();
        let duplicated = self
            .clipboard
            .iter()
            .filter_map(|id| scene.duplicate_node_into(*id, parent))
            .collect::<Vec<_>>();
        if duplicated.is_empty() {
            return;
        }
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = duplicated;
        self.request_document_frame();
    }

    pub fn select_children(&mut self, scene: &SceneGraph, parent: raf_core::scene::SceneNodeId) {
        let Some(node) = scene.get(parent) else {
            return;
        };
        let mut selected = Vec::new();
        let mut stack = node.children.clone();
        while let Some(id) = stack.pop() {
            if let Some(child) = scene.get(id) {
                if !child.name.is_empty() && child.visible {
                    selected.push(id);
                }
                stack.extend(child.children.iter().copied());
            }
        }
        self.game_viewport.selected = selected;
        self.request_overlay_frame();
    }

    pub fn reparent_to_root(&mut self, scene: &mut SceneGraph, id: raf_core::scene::SceneNodeId) {
        let before = scene.clone();
        if scene.reparent_node(id, None) {
            self.history.record_if_changed(before, scene);
            self.game_viewport.selected = vec![id];
            self.request_document_frame();
        }
    }

    pub fn reparent_nodes(
        &mut self,
        scene: &mut SceneGraph,
        ids: &[raf_core::scene::SceneNodeId],
        parent: Option<raf_core::scene::SceneNodeId>,
    ) -> bool {
        let before = scene.clone();
        if !scene.reparent_nodes_before(ids, parent, None) {
            return false;
        }
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = ids
            .iter()
            .copied()
            .filter(|id| scene.is_valid_node(*id))
            .collect();
        self.request_document_frame();
        true
    }

    pub fn ungroup(&mut self, scene: &mut SceneGraph, id: raf_core::scene::SceneNodeId) {
        let before = scene.clone();
        if scene.ungroup_node(id) {
            self.history.record_if_changed(before, scene);
            self.game_viewport
                .selected
                .retain(|selected| scene.is_valid_node(*selected));
            self.request_document_frame();
        }
    }

    pub fn select_all(&mut self, scene: &SceneGraph) {
        self.game_viewport.selected = scene
            .roots()
            .iter()
            .copied()
            .filter(|id| scene.is_valid_node(*id))
            .collect();
        self.request_overlay_frame();
    }

    /// Dispatches application shortcuts through the same registry used by
    /// menus, RafUI and automation. Text focus is supplied by the retained
    /// surface host; passive cursor hover never disables viewport shortcuts.
    pub fn dispatch_shortcuts(
        &mut self,
        input: &InputSnapshot,
        scene: &mut SceneGraph,
        text_input_focused: bool,
        project_open: bool,
    ) -> Vec<String> {
        let state = EditorCommandAvailability {
            project_open,
            project_type: Some(self.project_type),
            has_selection: !self.game_viewport.selected.is_empty(),
            text_input_focused,
            modal_open: false,
            viewport_active: true,
        };
        if !self.command_registry.enqueue_shortcut(input, state) {
            return Vec::new();
        }

        let envelopes = self.command_registry.drain().collect::<Vec<_>>();
        let mut dispatched = Vec::new();
        for envelope in envelopes {
            dispatched.push(envelope.id.clone());
            match envelope.id.as_str() {
                crate::application_menu::command::EDIT_UNDO => {
                    self.undo(scene);
                }
                crate::application_menu::command::EDIT_REDO => {
                    self.redo(scene);
                }
                crate::application_menu::command::EDIT_DUPLICATE => {
                    self.duplicate_selection(scene);
                }
                crate::application_menu::command::EDIT_COPY => {
                    self.copy_selected();
                }
                crate::application_menu::command::EDIT_PASTE => {
                    self.paste_clipboard(scene);
                }
                crate::application_menu::command::EDIT_DELETE => {
                    self.delete_selection(scene);
                }
                crate::application_menu::command::EDIT_SELECT_ALL => {
                    self.select_all(scene);
                }
                crate::application_menu::command::PROJECT_SAVE => {
                    self.request_ui_frame();
                }
                crate::application_menu::command::SEARCH_OPEN => {
                    self.request_ui_frame();
                }
                _ => {}
            }
        }
        dispatched
    }

    pub fn graphics(&self) -> &RenderRuntime {
        &self.graphics
    }

    pub fn project_type(&self) -> ProjectType {
        self.project_type
    }

    pub fn set_project_type(&mut self, project_type: ProjectType) {
        if self.project_type == project_type {
            return;
        }
        self.project_type = project_type;
        self.cached_game_canvas = None;
        self.graphics.request_frame(FrameInvalidation::UI);
    }

    pub fn reset_document(&mut self) {
        self.cached_game_canvas = None;
        self.game_viewport.selected.clear();
        self.game_viewport.bridge_mut().reset_isometric_view();
        self.history = SceneHistory::default();
        self.attached_undo = None;
        self.clipboard.clear();
        self.graphics.request_frame(
            FrameInvalidation::DOCUMENT | FrameInvalidation::UI | FrameInvalidation::CAMERA,
        );
    }

    pub fn graphics_mut(&mut self) -> &mut RenderRuntime {
        &mut self.graphics
    }

    /// Split the two native Game viewport resources for one Agent frame. The
    /// borrow is explicit so capture and scene commands cannot accidentally
    /// create a second renderer or a parallel viewport state.
    pub fn game_viewport_and_graphics_mut(
        &mut self,
    ) -> (&mut NativeGameViewportController, &mut RenderRuntime) {
        (&mut self.game_viewport, &mut self.graphics)
    }

    pub fn capture_last_viewport_rgba(&self) -> Result<SceneFrameCapture, String> {
        self.graphics.capture_last_scene_rgba()
    }

    pub fn layout(&self) -> EditorFrameLayout {
        self.layout
    }

    pub fn bottom_dock_is_collapsed(&self) -> bool {
        !self.layout_request.bottom_expanded
    }

    pub fn requested_bottom_dock_height(&self) -> f32 {
        self.layout_request.bottom_height
    }

    pub fn set_layout_request(&mut self, request: EditorLayoutRequest) {
        self.layout_request = request;
        self.layout = EditorFrameLayout::compute(request);
        self.cached_game_canvas = None;
        self.graphics.request_frame(FrameInvalidation::WINDOW);
    }

    /// Updates the authored width of the left workbench panel while keeping
    /// the canvas and right panel under the same layout solver.
    pub fn resize_left_panel(&mut self, width: f32) {
        self.layout_request.left_width = width.clamp(224.0, 760.0);
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    /// Updates the authored width of the right workbench panel while keeping
    /// the canvas and left panel under the same layout solver.
    pub fn resize_right_panel(&mut self, width: f32) {
        self.layout_request.right_width = width.clamp(260.0, 760.0);
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    /// Updates the expanded utility dock height without allowing it to erase
    /// the authoring canvas on small windows.
    pub fn resize_bottom_dock(&mut self, height: f32) {
        self.layout_request.bottom_height =
            height.clamp(EDITOR_DOCK_MIN_HEIGHT, EDITOR_DOCK_MAX_HEIGHT);
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn set_bottom_dock_collapsed(&mut self, collapsed: bool) {
        let expanded = !collapsed;
        if self.layout_request.bottom_expanded == expanded {
            return;
        }
        self.layout_request.bottom_expanded = expanded;
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn toggle_bottom_dock(&mut self) {
        self.layout_request.bottom_expanded = !self.layout_request.bottom_expanded;
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn toggle_left_panel(&mut self) {
        self.layout_request.left_visible = !self.layout_request.left_visible;
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn toggle_right_panel(&mut self) {
        self.layout_request.right_visible = !self.layout_request.right_visible;
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn resize(&mut self, target_size: [u32; 2], scale_factor: f32) {
        self.target_size = [target_size[0].max(1), target_size[1].max(1)];
        self.scale_factor = scale_factor.max(0.25);
        self.layout_request.logical_size = [
            self.target_size[0] as f32 / self.scale_factor,
            self.target_size[1] as f32 / self.scale_factor,
        ];
        self.layout = EditorFrameLayout::compute(self.layout_request);
        self.cached_game_canvas = None;
        self.graphics.request_frame(FrameInvalidation::WINDOW);
    }

    pub fn set_pacing_profile(&mut self, profile: FramePacingProfile) {
        self.graphics.set_frame_pacing_profile(profile);
        self.dynamic_resolution
            .reset(self.graphics.scheduler().budget());
    }

    pub fn apply_engine_settings(
        &mut self,
        settings: &EngineSettings,
        advanced_gpu_features_allowed: bool,
    ) {
        self.game_viewport.apply_engine_settings(settings);
        self.graphics.configure(
            settings.render_execution_policy,
            advanced_gpu_features_allowed,
        );
        self.graphics.set_frame_limit(settings.fps_limit);
        self.graphics
            .request_frame(FrameInvalidation::WINDOW | FrameInvalidation::UI);
    }

    pub fn set_frame_limit(&mut self, fps_limit: u32) {
        self.graphics.set_frame_limit(fps_limit);
    }

    pub fn update_game_input(
        &mut self,
        input: &InputSnapshot,
        scene: &mut SceneGraph,
    ) -> NativeViewportUpdate {
        let canvas = self.layout.canvas;
        let update = self.game_viewport.process_input(
            input,
            &mut self.input_router,
            ViewportInputRect::new(canvas.x, canvas.y, canvas.width, canvas.height),
            scene,
        );
        if let Some(before) = self.game_viewport.take_completed_scene_edit_snapshot() {
            self.history.record_if_changed(before, scene);
        }
        if update.scene_changed {
            self.cached_game_canvas = None;
            self.graphics.request_frame(FrameInvalidation::DOCUMENT);
        }
        if update.selection_changed {
            self.graphics
                .request_frame(FrameInvalidation::OVERLAY | FrameInvalidation::UI);
        }
        if update.needs_redraw {
            self.cached_game_canvas = None;
            self.graphics.request_frame(FrameInvalidation::CAMERA);
        }
        self.graphics.set_continuous_frame_reason(
            FrameInvalidation::POINTER_CAPTURE,
            self.input_router.has_pointer_capture(),
        );
        self.graphics
            .set_continuous_frame_reason(FrameInvalidation::CAMERA, update.camera_motion);
        update
    }

    fn request_document_frame(&mut self) {
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::DOCUMENT | FrameInvalidation::UI);
    }

    fn retain_valid_selection(&mut self, scene: &SceneGraph) {
        self.game_viewport
            .selected
            .retain(|id| scene.is_valid_node(*id));
    }

    fn duplicate_selection(&mut self, scene: &mut SceneGraph) {
        let selected = self.game_viewport.selected.clone();
        if selected.is_empty() {
            return;
        }
        let before = scene.clone();
        let mut duplicated = Vec::new();
        for id in selected {
            if let Some(new_id) = scene.duplicate_node(id) {
                duplicated.push(new_id);
            }
        }
        if duplicated.is_empty() {
            return;
        }
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = duplicated;
        self.request_document_frame();
    }

    fn paste_clipboard(&mut self, scene: &mut SceneGraph) {
        let before = scene.clone();
        let duplicated = self
            .clipboard
            .iter()
            .filter_map(|id| scene.duplicate_node(*id))
            .collect::<Vec<_>>();
        if duplicated.is_empty() {
            return;
        }
        self.history.record_if_changed(before, scene);
        self.game_viewport.selected = duplicated;
        self.request_document_frame();
    }

    fn delete_selection(&mut self, scene: &mut SceneGraph) {
        let selected = self.game_viewport.selected.clone();
        if selected.is_empty() {
            return;
        }
        let before = scene.clone();
        let mut removed = false;
        for id in selected {
            removed |= scene.remove_node(id);
        }
        if !removed {
            return;
        }
        self.history.record_if_changed(before, scene);
        self.retain_valid_selection(scene);
        self.request_document_frame();
    }

    /// Must run after every RafUI and canvas consumer has processed the same
    /// immutable snapshot. It releases captures only after owners saw mouse-up.
    pub fn finish_input_frame(&mut self, input: &InputSnapshot) {
        self.input_router.finish_frame(input);
        self.graphics.set_continuous_frame_reason(
            FrameInvalidation::POINTER_CAPTURE,
            self.input_router.has_pointer_capture(),
        );
    }

    pub fn request_ui_frame(&mut self) {
        self.graphics.request_frame(FrameInvalidation::UI);
    }

    pub fn request_canvas_frame(&mut self) {
        self.cached_game_canvas = None;
        self.graphics
            .request_frame(FrameInvalidation::DOCUMENT | FrameInvalidation::UI);
    }

    pub fn request_overlay_frame(&mut self) {
        self.graphics
            .request_frame(FrameInvalidation::OVERLAY | FrameInvalidation::UI);
    }

    pub fn request_animation_frame(&mut self) {
        self.graphics
            .request_frame(FrameInvalidation::ANIMATION | FrameInvalidation::UI);
    }

    pub fn set_window_focused(&mut self, focused: bool) {
        self.graphics.scheduler_mut().set_window_focused(focused);
        self.request_ui_frame();
    }

    pub fn set_continuous_ui_motion(&mut self, active: bool) {
        self.graphics
            .set_continuous_frame_reason(FrameInvalidation::ANIMATION, active);
    }

    pub fn set_continuous_text_input(&mut self, active: bool) {
        self.graphics
            .set_continuous_frame_reason(FrameInvalidation::UI, active);
    }

    /// Returns the measured presentation rate of the completed editor frames.
    /// This is not the configured FPS ceiling from EngineSettings.
    pub fn presented_fps(&self) -> f32 {
        self.graphics.snapshot().scheduler_metrics.presented_fps
    }

    pub fn begin_frame(&mut self, now_seconds: f64) -> bool {
        if self.active_frame.is_some() {
            return true;
        }
        self.active_frame = self.graphics.next_frame(now_seconds);
        if self.active_frame.is_some() {
            self.frame_started_at = Some(Instant::now());
        }
        self.active_frame.is_some()
    }

    pub fn render_game_canvas(&mut self, scene: &SceneGraph) -> Option<EditorCanvasLayer> {
        self.active_frame?;
        self.graphics
            .activate_surface(GraphicsSurfaceKind::SceneViewport);
        let canvas_target = self
            .layout
            .canvas
            .to_physical(self.scale_factor, self.target_size);
        if canvas_target.width == 0 || canvas_target.height == 0 {
            return None;
        }
        let scale = self.dynamic_resolution.scale().clamp(
            self.graphics.scheduler().budget().min_resolution_scale,
            self.graphics.scheduler().budget().max_resolution_scale,
        );
        let source_size = [
            ((canvas_target.width as f32) * scale).round().max(1.0) as u32,
            ((canvas_target.height as f32) * scale).round().max(1.0) as u32,
        ];
        let device_generation = self.graphics.snapshot().device_generation;
        let must_render = self
            .active_frame
            .is_none_or(|permit| canvas_requires_render(permit.reasons));
        if !must_render {
            if let Some(cached) = self.cached_game_canvas.as_ref().filter(|cached| {
                cached.device_generation == device_generation
                    && cached.layer.source_size == source_size
                    && cached.layer.target_rect == canvas_target
            }) {
                self.canvas_cache_hits = self.canvas_cache_hits.saturating_add(1);
                return Some(cached.layer.clone());
            }
        }
        self.canvas_renders = self.canvas_renders.saturating_add(1);
        let output = self
            .game_viewport
            .render(&mut self.graphics, scene, source_size);
        let layer = EditorCanvasLayer {
            output: Arc::new(output),
            source_size,
            target_rect: canvas_target,
        };
        self.cached_game_canvas = Some(CachedGameCanvas {
            device_generation,
            layer: layer.clone(),
        });
        Some(layer)
    }

    pub fn finish_frame(
        &mut self,
        presented_at_seconds: f64,
        frame_cpu_ms: f32,
        frame_gpu_ms: f32,
    ) {
        let Some(permit) = self.active_frame.take() else {
            return;
        };
        let measured_frame_cpu_ms = self
            .frame_started_at
            .take()
            .map(|started_at| started_at.elapsed().as_secs_f32() * 1000.0)
            .filter(|value| value.is_finite())
            .unwrap_or(frame_cpu_ms.max(0.0));
        self.last_frame_cpu_ms = measured_frame_cpu_ms;
        self.dynamic_resolution.update(
            self.graphics.scheduler().budget(),
            permit.activity,
            measured_frame_cpu_ms,
            frame_gpu_ms,
        );
        self.graphics.finish_frame(
            permit,
            presented_at_seconds,
            measured_frame_cpu_ms,
            frame_gpu_ms,
        );
    }

    pub fn cancel_frame(&mut self) {
        if let Some(permit) = self.active_frame.take() {
            self.frame_started_at = None;
            self.graphics.request_frame(permit.reasons);
        }
    }

    pub fn seconds_until_next_frame(&self, now_seconds: f64) -> Option<f64> {
        self.graphics
            .scheduler()
            .seconds_until_next_frame(now_seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::{InputKey, InputSnapshot};

    #[test]
    fn overlay_frame_reuses_the_scene_canvas() {
        assert!(!canvas_requires_render(
            FrameInvalidation::OVERLAY | FrameInvalidation::UI
        ));
        assert!(canvas_requires_render(
            FrameInvalidation::DOCUMENT | FrameInvalidation::UI
        ));
        assert!(canvas_requires_render(FrameInvalidation::EXPLICIT));
    }

    #[test]
    fn held_wasd_keeps_camera_frames_continuous() {
        let mut runtime = NativeEditorRuntime::new(EditorLayoutRequest::game([1440.0, 900.0]));
        let mut scene = SceneGraph::default();
        let mut input = InputSnapshot::default();
        runtime.game_viewport_mut().wasd_speed = 0.5;
        input.keys_down.insert(InputKey::W);
        input.delta_seconds = 1.0 / 60.0;
        let target_before = runtime.game_viewport().bridge().camera_target();

        let update = runtime.update_game_input(&input, &mut scene);
        let target_after = runtime.game_viewport().bridge().camera_target();

        assert!(update.camera_motion);
        assert!(
            target_before.distance(target_after) > 0.05,
            "a sub-unit WASD multiplier must still produce visible movement per frame"
        );
        assert!(runtime
            .graphics()
            .scheduler()
            .pending()
            .contains(FrameInvalidation::CAMERA));

        assert!(runtime.begin_frame(0.0));
        runtime.finish_input_frame(&input);
        runtime.finish_frame(0.0, 0.0, 0.0);
        assert!(runtime
            .graphics()
            .scheduler()
            .pending()
            .contains(FrameInvalidation::CAMERA));
    }
}
