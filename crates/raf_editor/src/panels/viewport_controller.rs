//! Native, backend-neutral Game viewport controller.
//!
//! This is the native interaction path for the editor viewport.
//! It owns camera/tool/selection gestures and records a renderer-owned gizmo;
//! window placement, RafUI chrome, and final presentation stay outside.

use glam::{Mat4, Quat, Vec3};
use raf_core::config::EngineSettings;
use raf_core::project::{EditorViewMode, GeometryDetailMode, ProjectSettings, TextureQualityMode};
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::{InputKey, InputOwner, InputRouter, InputSnapshot, PointerButton};
use raf_render::api_graphic_basic::command_list::BasicScreenTriangle;
use raf_render::api_graphic_basic::device::SceneFrameOutput;
use raf_render::bridge::{
    try_capture_camera, try_capture_gizmo, try_capture_viewport_tool, GizmoRenderSpec,
    MeshSelectMode, RenderRuntime, ViewportBridge, ViewportInputFrame, ViewportInputRect,
    ViewportNavigationConfig, ViewportPointerInput,
};
use raf_render::gizmo::{GizmoAxis, GizmoMode};
use raf_render::particles::ParticleRenderOptions;
use raf_render::render_config::RenderConfig;
use raf_render::scene_renderer::{RenderOptions, SceneRenderFrame, GRID_Y};
use raf_render::WorldStreamConfig;
use std::path::Path;

use crate::building_mode::{self, Aabb};
use crate::commands::game::GameViewportPort;
use crate::panels::viewport_compass::ViewportCompassTarget;

/// Ctrl-held translation quantum inherited from the single-axis gizmo path.
const CTRL_TRANSLATE_SNAP: f32 = 1.0;
/// Ctrl-held scale quantum inherited from the single-axis gizmo path.
const CTRL_SCALE_SNAP: f32 = 0.5;
/// Rotation snap shared by Ctrl holds and the Organized building style.
const ROTATE_SNAP_STEP: f32 = std::f32::consts::PI / 12.0;

#[derive(Debug, Clone, Copy)]
struct GestureMover {
    id: SceneNodeId,
    start_pos: Vec3,
    start_scale: Vec3,
    start_rotation: Vec3,
}

/// Pre-gesture state for every transformable selection member. All organized
/// building resolution recomputes final transforms from these starts so a
/// clamped frame never accumulates drift.
#[derive(Debug, Clone)]
struct GestureStart {
    movers: Vec<GestureMover>,
    /// Rotation/uniform-scale pivot: selection centroid for groups, the node
    /// itself for single selections.
    pivot: Vec3,
    /// Union world AABB over every solid under the movers at capture time.
    /// `None` when the selection carries no blocking geometry (empties).
    start_union: Option<Aabb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeViewportMode {
    View2d,
    #[default]
    View3d,
}

impl NativeViewportMode {
    pub const fn editor_view(self) -> EditorViewMode {
        match self {
            Self::View2d => EditorViewMode::View2d,
            Self::View3d => EditorViewMode::View3d,
        }
    }
}

impl From<EditorViewMode> for NativeViewportMode {
    fn from(mode: EditorViewMode) -> Self {
        match mode {
            EditorViewMode::View2d => Self::View2d,
            EditorViewMode::View3d => Self::View3d,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeViewportRenderStyle {
    #[default]
    Solid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeViewportEditMode {
    #[default]
    Object,
    /// Focused Tab Mode topology editing. Everything outside the edited mesh
    /// is dimmed and entity selection is locked to that node.
    Mesh,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NativeViewportUpdate {
    pub scene_changed: bool,
    pub selection_changed: bool,
    pub gesture_finished: bool,
    pub needs_redraw: bool,
    /// True while the camera has a persistent navigation input (keyboard fly
    /// keys or a held orbit/pan button). The runtime uses this to keep frames
    /// flowing even when the OS emits no repeated key event.
    pub camera_motion: bool,
    /// Tab Mode lifecycle edges, consumed by the presentation layer to show
    /// the controls hint once instead of every frame.
    pub mesh_session_started: bool,
    pub mesh_session_finished: bool,
    pub mesh_session_cancelled: bool,
}

#[derive(Debug, Clone)]
struct FreeDragMember {
    id: SceneNodeId,
    local_start: Vec3,
}

#[derive(Debug, Clone)]
struct FreeDragGesture {
    pointer_start: [f32; 2],
    plane_origin: Vec3,
    members: Vec<FreeDragMember>,
    moved: bool,
}

pub struct NativeGameViewportController {
    pub mode: NativeViewportMode,
    pub render_style: NativeViewportRenderStyle,
    pub edit_mode: NativeViewportEditMode,
    pub edit_notice: bool,
    /// Constrained axis while dragging mesh vertices (0=X, 1=Y, 2=Z).
    pub mesh_axis: Option<usize>,
    /// Short human-readable status for the Tab Mode toolbar.
    pub mesh_status: Option<String>,
    /// Entity camera used as the live viewport viewpoint.
    pub look_through: Option<SceneNodeId>,
    pub(crate) placement_notice: Option<&'static str>,
    pub(crate) placement: Option<crate::authoring_placement::AssetPlacement>,
    pub selected: Vec<SceneNodeId>,
    pub grid_visible: bool,
    pub snap_to_grid: bool,
    pub grid_spacing: f32,
    pub grid_load_distance: f32,
    pub show_labels: bool,
    pub solid_show_surface_edges: bool,
    pub solid_xray_mode: bool,
    pub solid_face_tonality: bool,
    pub lighting_preview_neutral: bool,
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    pub invert_ws: bool,
    pub focus_lock_enabled: bool,
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub wasd_speed: f32,
    pub wasd_speed_boost: f32,
    pub uniform_scale_by_default: bool,
    /// Engine setting: render and honor the gizmo while several entities are
    /// selected. When false the gizmo stays single-selection only.
    pub multi_select_gizmo_enabled: bool,
    /// Percentage of distance-based gizmo growth. Zero keeps a fixed handle
    /// size; one hundred restores the full camera-distance response.
    pub gizmo_growth_scale: f32,
    /// Global editor-only particle preview controls.
    pub particle_preview: raf_core::scene::ParticlePreviewSettings,
    /// Last accepted viewport delta used by particle simulation.
    pub last_frame_seconds: f32,
    /// True while the camera is being orbited, panned, zoomed or flown. Editor
    /// overlays whose screen geometry depends on the camera are suppressed for
    /// these frames so the retained AGB scene packet can be reused.
    pub camera_navigation_active: bool,
    /// Keeps navigation sticky through brief input gaps (key repeat, focus
    /// reconciliation) so gizmos/helpers do not flicker and force a full scene
    /// packet rebuild in the middle of a gesture.
    navigation_grace_seconds: f32,
    /// Project building style: quantized, collision-aware authoring.
    pub building_organized: bool,
    /// Fixed metric quantum used by the organized style (meters).
    pub building_snap_step: f32,
    render_config: RenderConfig,
    geometry_detail: GeometryDetailMode,
    texture_quality: TextureQualityMode,
    scene_packet_reuse: bool,
    world_stream_config: WorldStreamConfig,
    bridge: ViewportBridge,
    free_drag: Option<FreeDragGesture>,
    scene_edit_snapshot: Option<SceneGraph>,
    completed_scene_edit_snapshot: Option<SceneGraph>,
    /// Scene state captured when Tab Mode opened. Escape restores it; Apply
    /// keeps the live document and discards it.
    mesh_entry_snapshot: Option<SceneGraph>,
    mesh_last_exit_cancelled: bool,
    mesh_last_secondary_click: Option<(f64, [f32; 2])>,
    viewport_active: bool,
    gesture_start: Option<GestureStart>,
    /// True when the latest organized-resolution frame had to back the
    /// gesture off against a wall. Drives the contact outline overlay.
    collision_contact: bool,
    camera_helper_pixel_scale: f32,
    camera_helpers: raf_render::bridge::camera_helpers::CameraHelperCache,
}

impl Default for NativeGameViewportController {
    fn default() -> Self {
        Self {
            mode: NativeViewportMode::View3d,
            render_style: NativeViewportRenderStyle::Solid,
            edit_mode: NativeViewportEditMode::Object,
            edit_notice: false,
            mesh_axis: None,
            mesh_status: None,
            look_through: None,
            placement_notice: None,
            placement: None,
            selected: Vec::new(),
            grid_visible: true,
            snap_to_grid: true,
            grid_spacing: 1.0,
            grid_load_distance: 15.0,
            show_labels: true,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,
            lighting_preview_neutral: false,
            invert_mouse_x: false,
            invert_mouse_y: true,
            invert_ws: false,
            focus_lock_enabled: true,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            wasd_speed: 1.0,
            wasd_speed_boost: 1.0,
            uniform_scale_by_default: false,
            multi_select_gizmo_enabled: true,
            gizmo_growth_scale: 0.0,
            building_organized: false,
            building_snap_step: 1.0,
            render_config: RenderConfig::editor_lightweight(),
            particle_preview: Default::default(),
            last_frame_seconds: 1.0 / 60.0,
            camera_navigation_active: false,
            navigation_grace_seconds: 0.0,
            geometry_detail: GeometryDetailMode::Adaptive,
            texture_quality: TextureQualityMode::Adaptive,
            scene_packet_reuse: true,
            world_stream_config: WorldStreamConfig::default(),
            bridge: ViewportBridge::default(),
            free_drag: None,
            scene_edit_snapshot: None,
            completed_scene_edit_snapshot: None,
            mesh_entry_snapshot: None,
            mesh_last_exit_cancelled: false,
            mesh_last_secondary_click: None,
            viewport_active: true,
            gesture_start: None,
            collision_contact: false,
            camera_helper_pixel_scale: 1.0,
            camera_helpers: Default::default(),
        }
    }
}

impl NativeGameViewportController {
    /// Physical/adaptive scene pixels per logical authoring point.
    pub fn set_camera_helper_pixel_scale(&mut self, scale: f32) {
        self.camera_helper_pixel_scale = scale.clamp(0.1, 8.0);
    }
    pub fn set_project_asset_root(&mut self, project_root: Option<&Path>) {
        self.bridge.set_project_asset_root(project_root);
    }

    /// Decodes the visible project textures on worker threads before the first
    /// textured viewport frame. Loading used to decode each 1024px image on the
    /// UI thread as it became visible, which produced a burst of hitches while
    /// the gray scene filled in.
    pub fn prewarm_scene_textures(
        &mut self,
        scene: &raf_core::scene::graph::SceneGraph,
        quality: raf_core::project::TextureQualityMode,
    ) {
        self.bridge.prewarm_scene_textures(scene, quality);
    }

    pub fn bridge(&self) -> &ViewportBridge {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut ViewportBridge {
        &mut self.bridge
    }

    pub fn set_active(&mut self, active: bool) {
        self.viewport_active = active;
    }

    /// Applies the authored editor camera view without changing scene data.
    /// The 2D path keeps the fixed orthographic orientation and its own pan/zoom.
    pub fn set_editor_view_mode(&mut self, mode: EditorViewMode) {
        let next = NativeViewportMode::from(mode);
        if self.mode == next {
            return;
        }
        self.mode = next;
        self.bridge
            .update_camera(next == NativeViewportMode::View2d);
    }

    pub fn editor_view_mode(&self) -> EditorViewMode {
        self.mode.editor_view()
    }

    /// Render through an entity camera while keeping the editor orbit camera
    /// untouched. Escape or a removed/invalid camera returns to editor view.
    pub fn set_look_through(&mut self, scene: &SceneGraph, id: Option<SceneNodeId>) {
        self.look_through = id.filter(|id| {
            scene
                .get(*id)
                .is_some_and(|node| node.game_camera.is_some())
        });
        if self.look_through.is_none() && id.is_some() {
            self.mesh_status = Some("Look-through requires an entity with a Game camera.".into());
        }
    }

    pub fn look_through_active(&self) -> bool {
        self.look_through.is_some()
    }

    fn look_through_camera(&self, scene: &SceneGraph) -> Option<raf_render::Camera> {
        let id = self.look_through?;
        let node = scene.get(id)?;
        let game_camera = node.game_camera.as_ref()?;
        let world = scene.world_matrix(id);
        let position = world.transform_point3(Vec3::ZERO);
        let forward = world.transform_vector3(-Vec3::Z);
        let up = world.transform_vector3(Vec3::Y);
        if !position.is_finite() || !forward.is_finite() || !up.is_finite() {
            return None;
        }
        Some(raf_render::Camera {
            position,
            target: position + forward,
            up,
            mode: if game_camera.orthographic {
                raf_render::camera::CameraMode::Orthographic
            } else {
                raf_render::camera::CameraMode::Perspective
            },
            fov: game_camera.fov_degrees,
            near: game_camera.near,
            far: game_camera.far,
            ortho_scale: game_camera.ortho_scale,
        })
    }

    /// Applies engine-wide viewport preferences to the live native controller.
    /// The settings surface owns the draft, while this controller owns the
    /// values consumed by camera and gizmo input, so they must be synchronized
    /// explicitly before processing a frame.
    pub fn apply_engine_settings(&mut self, settings: &EngineSettings) {
        self.grid_visible = settings.grid_visible;
        self.snap_to_grid = settings.snap_to_grid;
        self.grid_spacing = settings.grid_size.clamp(0.1, 10.0);
        self.grid_load_distance = settings.grid_load_distance.clamp(0.0, 500.0);
        self.show_labels = settings.show_viewport_labels;
        self.render_style = NativeViewportRenderStyle::Solid;
        self.solid_show_surface_edges = settings.solid_show_surface_edges;
        self.solid_xray_mode = settings.solid_xray_mode;
        self.solid_face_tonality = settings.solid_face_tonality;
        self.render_config = RenderConfig::editor_lightweight();
        self.invert_mouse_x = settings.invert_mouse_x;
        self.invert_mouse_y = settings.invert_mouse_y;
        self.invert_ws = settings.invert_ws;
        self.focus_lock_enabled = settings.focus_lock_enabled;
        self.wasd_speed = settings.wasd_speed.clamp(0.05, 5.0);
        self.move_sensitivity = settings.move_gizmo_sensitivity.clamp(0.25, 4.0);
        self.rotate_sensitivity = settings.rotate_gizmo_sensitivity.clamp(0.25, 4.0);
        self.scale_sensitivity = settings.scale_gizmo_sensitivity.clamp(0.25, 4.0);
        self.uniform_scale_by_default = settings.uniform_scale_by_default;
        self.multi_select_gizmo_enabled = settings.multi_select_gizmo_enabled;
        self.gizmo_growth_scale = settings.gizmo_growth_scale.clamp(0.0, 100.0);
        self.particle_preview = settings.particle_preview.clone();
        self.texture_quality = settings.texture_quality;
        self.scene_packet_reuse = settings.scene_packet_reuse;
    }

    /// Applies project-local runtime/render policy after the global engine
    /// preferences. Keeping this merge here makes Project Settings affect the
    /// same viewport path that already consumes the engine settings instead
    /// of leaving the controls as persisted-but-inert metadata.
    pub fn apply_project_settings(&mut self, settings: &ProjectSettings) {
        self.building_organized =
            settings.building_style == raf_core::project::BuildingStyle::Organized;
        self.building_snap_step = settings.building_snap_step.clamp(0.5, 2.0);

        // The editor intentionally stays on one lightweight profile during
        // renderer stabilization. Project presets remain persisted for future
        // runtime use but do not switch viewport behavior yet.
        self.render_config = RenderConfig::editor_lightweight();
        self.geometry_detail = settings.geometry_detail;
        self.world_stream_config = WorldStreamConfig::from_project_settings(settings);
    }

    pub fn process_input(
        &mut self,
        input: &InputSnapshot,
        router: &mut InputRouter,
        rect: ViewportInputRect,
        scene: &mut SceneGraph,
    ) -> NativeViewportUpdate {
        if self.placement.is_some() {
            return self.update_asset_placement(input, rect, scene);
        }
        self.selected.retain(|id| scene.is_valid_node(*id));
        let edit_mode_before = self.edit_mode;
        let mut update = NativeViewportUpdate::default();
        let size = [rect.size[0].max(1.0), rect.size[1].max(1.0)];
        if self.look_through.is_some() {
            let valid = self
                .look_through
                .and_then(|id| scene.get(id))
                .is_some_and(|node| node.game_camera.is_some());
            if !valid || input.key_pressed(InputKey::Escape) {
                self.look_through = None;
                update.needs_redraw = true;
            }
        }

        // Tool hotkeys are evaluated before camera movement, but never while
        // a primary gesture or command chord owns the frame. T is Scale; S is
        // permanently left to WASD navigation.
        let before_capture =
            ViewportInputFrame::from_snapshot(input, router, rect, self.viewport_active);
        update.scene_changed |= self.apply_tool_shortcuts(before_capture, scene);

        self.bridge
            .set_picking_policy(raf_render::bridge::PickingPolicy::for_render_config(
                &self.render_config,
            ));
        self.bridge
            .update_camera(self.mode == NativeViewportMode::View2d);
        let look_camera = self.look_through_camera(scene);
        let view_proj = look_camera
            .as_ref()
            .map(|camera| self.bridge.view_projection_for(camera, size[0], size[1]))
            .unwrap_or_else(|| self.bridge.view_projection(size[0], size[1]));
        let previous_hover = (
            self.bridge.highlighted_gizmo_axis(),
            self.bridge.highlighted_gizmo_scale_sign(),
        );

        if self.edit_mode == NativeViewportEditMode::Mesh {
            self.bridge.update_transform_hover(
                scene,
                None,
                &view_proj,
                [0.0, 0.0],
                size[0],
                size[1],
            );
            if let Some(pointer) = before_capture.pointer_local {
                self.bridge
                    .edit_session_mut()
                    .update_hover(scene, &view_proj, size[0], size[1], pointer);
                update.needs_redraw = true;
            }
        } else if self.edit_mode == NativeViewportEditMode::Object {
            if let Some(pointer) = before_capture.pointer_local {
                if self.selected.len() == 1 {
                    self.bridge.update_transform_hover_scaled(
                        scene,
                        self.selected.first().copied(),
                        &view_proj,
                        pointer,
                        size[0],
                        size[1],
                        self.gizmo_scale_for_scene(scene, &view_proj, size),
                    );
                } else if let Some((origin, scale_ref)) = self.gizmo_anchor(scene) {
                    self.bridge.update_transform_hover_world(
                        origin,
                        scale_ref,
                        &view_proj,
                        pointer,
                        size[0],
                        size[1],
                        self.gizmo_scale_for_scene(scene, &view_proj, size),
                    );
                } else {
                    self.bridge.update_transform_hover(
                        scene,
                        None,
                        &view_proj,
                        [0.0, 0.0],
                        size[0],
                        size[1],
                    );
                }
            } else {
                // CursorLeft must clear a stale gizmo highlight, but doing so
                // is still only a one-shot visual invalidation below. A
                // passive pointer leaving the viewport must never become a
                // continuous scene redraw source.
                self.bridge.update_transform_hover(
                    scene,
                    None,
                    &view_proj,
                    [0.0, 0.0],
                    size[0],
                    size[1],
                );
            }
        } else {
            self.bridge.update_transform_hover(
                scene,
                None,
                &view_proj,
                [0.0, 0.0],
                size[0],
                size[1],
            );
        }

        let mut mesh_poked = false;
        if self.edit_mode == NativeViewportEditMode::Mesh
            && input.button_pressed(PointerButton::Secondary)
        {
            if let Some(pointer) = input.pointer_position.map(|point| rect.to_local(point)) {
                let double = self
                    .mesh_last_secondary_click
                    .is_some_and(|(time, previous)| {
                        let dx = pointer[0] - previous[0];
                        let dy = pointer[1] - previous[1];
                        input.time_seconds - time <= 0.35 && (dx * dx + dy * dy).sqrt() <= 6.0
                    });
                self.mesh_last_secondary_click = Some((input.time_seconds, pointer));
                if double {
                    mesh_poked = self.mesh_poke_at(scene, &view_proj, size, pointer);
                    if mesh_poked {
                        update.scene_changed = true;
                        update.needs_redraw = true;
                    }
                }
            }
        }
        if input.button_pressed(PointerButton::Secondary) && !mesh_poked {
            let _ = try_capture_camera(router, input, rect, PointerButton::Secondary);
        }
        if input.button_pressed(PointerButton::Middle) {
            let _ = try_capture_camera(router, input, rect, PointerButton::Middle);
        }

        if input.button_pressed(PointerButton::Primary)
            && self.edit_mode == NativeViewportEditMode::Mesh
        {
            if try_capture_viewport_tool(router, input, rect) {
                if let Some(pointer) = input.pointer_position.map(|point| rect.to_local(point)) {
                    let selected = self.selected.first().copied();
                    self.bridge.handle_edit_selection_click(
                        scene,
                        selected,
                        &view_proj,
                        size[0],
                        size[1],
                        pointer,
                        input.modifiers.shift,
                    );
                    self.bridge
                        .begin_edit_drag(scene, selected, &view_proj, size[0], size[1], pointer);
                    if self.bridge.edit_session().is_dragging() {
                        self.begin_scene_edit(scene);
                    }
                    update.selection_changed = true;
                    update.needs_redraw = true;
                    self.refresh_mesh_status(scene);
                }
            }
        } else if input.button_pressed(PointerButton::Primary) {
            let anchor = self.gizmo_anchor(scene);
            let gizmo_hit = self.edit_mode == NativeViewportEditMode::Object
                && self.bridge.highlighted_gizmo_axis() != GizmoAxis::None
                && anchor.is_some();
            if try_capture_gizmo(router, input, rect, gizmo_hit) {
                if let (Some(pointer), Some((origin, scale_ref))) = (
                    input.pointer_position.map(|point| rect.to_local(point)),
                    anchor,
                ) {
                    self.begin_scene_edit(scene);
                    self.capture_gesture_start(scene);
                    let multi = self
                        .gesture_start
                        .as_ref()
                        .is_some_and(|gesture| gesture.movers.len() > 1);
                    if multi {
                        self.bridge.begin_transform_drag_world(
                            origin,
                            scale_ref,
                            &view_proj,
                            pointer,
                            size[0],
                            size[1],
                            self.gizmo_scale_for_scene(scene, &view_proj, size),
                        );
                    } else {
                        self.bridge.begin_transform_drag_scaled(
                            scene,
                            self.selected.first().copied(),
                            &view_proj,
                            pointer,
                            size[0],
                            size[1],
                            self.gizmo_scale_for_scene(scene, &view_proj, size),
                        );
                    }
                }
            } else if try_capture_viewport_tool(router, input, rect) {
                if let Some(pointer) = input.pointer_position.map(|point| rect.to_local(point)) {
                    let picked = self
                        .bridge
                        .pick_entity(scene, &view_proj, pointer[0], pointer[1], size[0], size[1]);
                    update.selection_changed |=
                        self.apply_pick_selection(picked, input.modifiers.shift);
                    self.begin_free_drag(scene, pointer, picked);
                }
            }
        }

        let routed = ViewportInputFrame::from_snapshot(input, router, rect, self.viewport_active);
        update.camera_motion = routed.has_camera_motion();
        update.scene_changed |= self.update_primary_gesture(routed, scene, &view_proj, size);
        update.needs_redraw |= self.update_camera(routed);

        if routed.button_released(PointerButton::Primary)
            || (!input.button_down(PointerButton::Primary)
                && matches!(
                    router.pointer_owner(PointerButton::Primary),
                    Some(InputOwner::ViewportGizmo | InputOwner::ViewportTool)
                ))
        {
            let mesh_drag = self.edit_mode == NativeViewportEditMode::Mesh
                && self.bridge.edit_session().is_dragging()
                && self.scene_edit_snapshot.is_some();
            let changed = self.free_drag.as_ref().is_some_and(|drag| drag.moved)
                || self.bridge.active_drag_axis() != GizmoAxis::None
                || mesh_drag;
            self.bridge.end_transform_drag();
            self.bridge.clear_edit_drag_state();
            self.free_drag = None;
            self.gesture_start = None;
            self.collision_contact = false;
            if changed {
                self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
                update.gesture_finished = true;
            } else {
                self.scene_edit_snapshot = None;
            }
            if self.edit_mode == NativeViewportEditMode::Mesh {
                self.refresh_mesh_status(scene);
            }
        }

        let camera_transition_was_active = self.bridge.has_camera_transition();
        let camera_transition_active = self.bridge.update_camera_transitions(input.delta_seconds);
        update.camera_motion |= camera_transition_active;
        if camera_transition_active {
            self.camera_navigation_active = true;
        }
        let hover_changed = previous_hover.0 != self.bridge.highlighted_gizmo_axis()
            || (previous_hover.1 - self.bridge.highlighted_gizmo_scale_sign()).abs() > f32::EPSILON;
        update.needs_redraw |= update.scene_changed
            || update.selection_changed
            // Gizmos are part of the retained scene canvas today, so a real
            // hover transition needs one refresh. Repeated movement over the
            // same handle stays cached.
            || hover_changed
            || routed.requires_continuous_redraw()
            // Keep one final frame queued so the renderer and compass both
            // receive the exact settled orientation.
            || camera_transition_was_active
            || camera_transition_active;
        update.mesh_session_started = edit_mode_before != NativeViewportEditMode::Mesh
            && self.edit_mode == NativeViewportEditMode::Mesh;
        update.mesh_session_finished = edit_mode_before == NativeViewportEditMode::Mesh
            && self.edit_mode != NativeViewportEditMode::Mesh;
        update.mesh_session_cancelled =
            update.mesh_session_finished && self.mesh_last_exit_cancelled;
        self.mesh_last_exit_cancelled = false;
        self.last_frame_seconds = input.delta_seconds.clamp(1.0 / 240.0, 0.1);
        update
    }

    pub fn build_scene_frame(&mut self, scene: &SceneGraph, size: [u32; 2]) -> SceneRenderFrame {
        let width = size[0].max(1);
        let height = size[1].max(1);
        let look_camera = self.look_through_camera(scene);
        if look_camera.is_none() {
            self.bridge
                .update_camera(self.mode == NativeViewportMode::View2d);
        }
        let light_dir = raf_render::scene_renderer::SCENE_LIGHT_DIRECTION.normalize();
        let mut frame = self.bridge.build_scene_frame_with_camera(
            scene,
            width as f32,
            height as f32,
            &self.selected,
            raf_render::scene_renderer::SCENE_BACKGROUND,
            light_dir,
            self.render_options(),
            false,
            look_camera.as_ref(),
        );
        if look_camera.is_some() {
            self.append_safe_frame(&mut frame);
        }
        if self.edit_mode == NativeViewportEditMode::Mesh {
            self.append_mesh_edit_overlay(scene, &mut frame);
        } else if self.bridge.gizmo().visible && !self.camera_navigation_active {
            self.camera_helpers.append_to(
                scene,
                &mut frame,
                &self.selected,
                self.camera_helper_pixel_scale,
            );
        }
        if self.edit_mode == NativeViewportEditMode::Object && !self.camera_navigation_active {
            self.append_renderer_owned_gizmo(scene, &mut frame);
        }
        self.append_collision_contact_outline(scene, &mut frame);
        if let Some(placement) = &self.placement {
            placement.append(&mut frame);
        }
        frame
    }

    pub fn render(
        &mut self,
        runtime: &mut RenderRuntime,
        scene: &SceneGraph,
        size: [u32; 2],
    ) -> SceneFrameOutput {
        let frame = self.build_scene_frame(scene, size);
        runtime.render_scene_frame(&frame)
    }

    fn update_asset_placement(
        &mut self,
        input: &InputSnapshot,
        rect: ViewportInputRect,
        scene: &mut SceneGraph,
    ) -> NativeViewportUpdate {
        let mut update = NativeViewportUpdate {
            needs_redraw: true,
            ..Default::default()
        };
        if input.key_pressed(InputKey::Escape) || !input.window_focused {
            self.placement = None;
            self.placement_notice = Some("assets.placement_cancelled");
            return update;
        }
        let mut placement = self.placement.take().unwrap();
        let point = input.pointer_position.filter(|p| rect.contains(*p));
        placement.valid = false;
        if let Some(pointer) = point {
            self.bridge
                .update_camera(self.mode == NativeViewportMode::View2d);
            let local = rect.to_local(pointer);
            let vp = self.bridge.view_projection(rect.size[0], rect.size[1]);
            let hit = self
                .bridge
                .pick_surface(scene, &vp, local, rect.size)
                .or_else(|| {
                    let (origin, dir) = raf_render::math::transform::screen_to_world_ray(
                        local[0],
                        local[1],
                        rect.size[0],
                        rect.size[1],
                        &vp.inverse(),
                    )?;
                    if dir.y.abs() < 1e-6 {
                        return None;
                    }
                    // The grid's visual depth offset is not the construction floor.
                    let t = -origin.y / dir.y;
                    (t >= 0.0).then_some((origin + dir * t, Vec3::Y))
                });
            if let Some((point, normal)) = hit {
                placement.support_at(point, normal);
            }
            placement.pressed |= input.button_pressed(PointerButton::Primary);
        }
        if input.button_released(PointerButton::Primary) && placement.pressed {
            if placement.valid {
                let before = scene.clone();
                match placement.commit(scene) {
                    Ok(id) => {
                        self.selected = vec![id];
                        self.completed_scene_edit_snapshot = Some(before);
                        update.scene_changed = true;
                        update.selection_changed = true;
                        update.gesture_finished = true;
                        self.placement_notice = Some("assets.placed");
                    }
                    Err(_) => {
                        *scene = before;
                        self.placement_notice = Some("assets.invalid_parent");
                    }
                }
            } else {
                self.placement_notice = Some("assets.placement_cancelled");
            }
        } else {
            self.placement = Some(placement);
        }
        update
    }

    pub fn take_completed_scene_edit_snapshot(&mut self) -> Option<SceneGraph> {
        self.completed_scene_edit_snapshot.take()
    }

    pub fn set_gizmo_mode(&mut self, mode: GizmoMode) {
        self.free_drag = None;
        self.bridge.end_transform_drag();
        self.bridge.gizmo_mut().visible = true;
        self.bridge.set_gizmo_mode(mode);
    }

    pub fn focus_selection(&mut self, scene: &SceneGraph) {
        self.bridge.focus_selected(
            scene,
            self.selected.first().copied(),
            self.mode == NativeViewportMode::View2d,
        );
    }

    pub fn transform_drag_active(&self) -> bool {
        self.gesture_active()
    }

    /// Applies an orientation-gizmo snap through the same live camera bridge
    /// used by mouse orbiting and keyboard navigation.
    pub fn snap_compass_target(&mut self, target: ViewportCompassTarget) {
        if let Some(axis) = target.axis() {
            self.bridge.snap_view_to_axis(axis);
        } else {
            self.bridge.reset_isometric_view();
        }
        self.bridge
            .update_camera(self.mode == NativeViewportMode::View2d);
    }

    fn apply_tool_shortcuts(&mut self, input: ViewportInputFrame, scene: &mut SceneGraph) -> bool {
        if !input.keyboard_enabled || input.button_down(PointerButton::Primary) {
            return false;
        }
        if self.edit_mode == NativeViewportEditMode::Mesh {
            return self.apply_mesh_shortcuts(input, scene);
        }
        if input.modifiers.command_modifier() {
            return false;
        }
        if input.key_pressed(InputKey::G) {
            self.set_gizmo_mode(GizmoMode::Translate);
        } else if input.key_pressed(InputKey::R) {
            self.set_gizmo_mode(GizmoMode::Rotate);
        } else if input.key_pressed(InputKey::T) {
            self.set_gizmo_mode(GizmoMode::Scale);
        } else if input.key_pressed(InputKey::C) {
            self.bridge.gizmo_mut().visible = false;
        } else if input.key_pressed(InputKey::F) && self.focus_lock_enabled {
            self.focus_selection(scene);
        } else if input.key_pressed(InputKey::Tab) && !self.selected.is_empty() {
            return self.enter_mesh_edit(scene);
        }
        if input.key_pressed(InputKey::Numpad2) {
            self.mode = NativeViewportMode::View2d;
        } else if input.key_pressed(InputKey::Numpad3) {
            self.mode = NativeViewportMode::View3d;
        }
        false
    }

    fn apply_mesh_shortcuts(&mut self, input: ViewportInputFrame, scene: &mut SceneGraph) -> bool {
        if input.key_pressed(InputKey::Tab) {
            return self.finish_mesh_edit(scene, true);
        }
        if input.key_pressed(InputKey::Escape) {
            return self.finish_mesh_edit(scene, false);
        }
        if input.key_pressed(InputKey::Digit1) {
            self.set_mesh_select_mode(MeshSelectMode::Vertex);
        } else if input.key_pressed(InputKey::Digit2) {
            self.set_mesh_select_mode(MeshSelectMode::Edge);
        } else if input.key_pressed(InputKey::Digit3) {
            self.set_mesh_select_mode(MeshSelectMode::Face);
        }
        if input.key_pressed(InputKey::G) && !input.modifiers.command_modifier() {
            self.mesh_axis = None;
            self.refresh_mesh_status(scene);
        }
        if input.modifiers.shift {
            if input.key_pressed(InputKey::X) {
                self.mesh_axis = Some(0);
            } else if input.key_pressed(InputKey::Y) {
                self.mesh_axis = Some(1);
            } else if input.key_pressed(InputKey::Z) {
                self.mesh_axis = Some(2);
            }
        }
        if input.key_pressed(InputKey::A) && !input.modifiers.command_modifier() {
            self.bridge.edit_session_mut().select_all(scene);
            self.refresh_mesh_status(scene);
        } else if input.key_pressed(InputKey::Backspace) {
            self.bridge.edit_session_mut().clear_selection();
            self.refresh_mesh_status(scene);
        }
        if input.key_pressed(InputKey::E) && !input.modifiers.command_modifier() {
            return self.mesh_extrude(scene);
        }
        if input.key_pressed(InputKey::I) && !input.modifiers.command_modifier() {
            return self.mesh_inset(scene);
        }
        if input.key_pressed(InputKey::R) && input.modifiers.command_modifier() {
            return self.mesh_subdivide(scene);
        }
        if input.key_pressed(InputKey::X) || input.key_pressed(InputKey::Delete) {
            return self.mesh_delete_faces(scene);
        }
        false
    }

    fn set_mesh_select_mode(&mut self, mode: MeshSelectMode) {
        self.bridge.edit_session_mut().set_mode(mode);
        self.mesh_status = Some(format!("Select: {}", mode.slug()));
    }

    fn refresh_mesh_status(&mut self, scene: &SceneGraph) {
        let session = self.bridge.edit_session();
        let vertices = session.selected_vertices.len();
        let faces = session.selected_faces.len();
        let total_vertices = session.mesh(scene).map_or(0, |mesh| mesh.vertices.len());
        let total_faces = session.mesh(scene).map_or(0, |mesh| mesh.face_count());
        let axis = match self.mesh_axis {
            Some(0) => " | axis X",
            Some(1) => " | axis Y",
            Some(2) => " | axis Z",
            _ => "",
        };
        self.mesh_status = Some(format!(
            "{}V selected | {}F selected | {}V / {}F total{}",
            vertices, faces, total_vertices, total_faces, axis
        ));
    }

    fn enter_mesh_edit(&mut self, scene: &mut SceneGraph) -> bool {
        if self.selected.len() != 1 {
            self.edit_notice = true;
            return false;
        }
        let Some(id) = self.selected.first().copied() else {
            return false;
        };
        let Some(node) = scene.get(id) else {
            return false;
        };
        if node.is_folder {
            self.mesh_status = Some("Folders have no mesh topology to edit.".into());
            self.edit_notice = true;
            return false;
        }
        if node.mesh_asset.is_some() && node.editable_mesh.is_none() {
            self.mesh_status =
                Some("Imported meshes are not editable yet; convert or use a primitive.".into());
            self.edit_notice = true;
            return false;
        }
        let primitive = node.primitive;
        self.mesh_entry_snapshot = Some(scene.clone());
        let mut changed = false;
        if node.editable_mesh.is_none() {
            let mut data =
                raf_render::editable::EditableMesh::from_primitive(primitive).to_scene_data();
            data.touch();
            if let Some(node) = scene.get_mut(id) {
                node.editable_mesh = Some(data);
            }
            changed = true;
        }
        self.edit_mode = NativeViewportEditMode::Mesh;
        self.edit_notice = false;
        self.mesh_axis = None;
        self.look_through = None;
        self.bridge
            .prepare_selected_edit_mesh(scene, self.selected.first().copied());
        self.bridge
            .edit_session_mut()
            .set_mode(MeshSelectMode::Vertex);
        self.refresh_mesh_status(scene);
        changed
    }

    fn finish_mesh_edit(&mut self, scene: &mut SceneGraph, accept: bool) -> bool {
        let mut changed = false;
        if !accept {
            if let Some(snapshot) = self.mesh_entry_snapshot.take() {
                *scene = snapshot;
                changed = true;
            }
        } else {
            self.mesh_entry_snapshot = None;
        }
        self.mesh_last_exit_cancelled = !accept;
        self.mesh_last_secondary_click = None;
        self.edit_mode = NativeViewportEditMode::Object;
        self.mesh_axis = None;
        self.mesh_status = None;
        self.edit_notice = false;
        self.bridge.clear_edit_drag_state();
        self.bridge.edit_session_mut().release();
        changed
    }

    fn mesh_face_selection(&self) -> Vec<u32> {
        self.bridge.edit_session().selected_faces.clone()
    }

    fn mesh_extrude(&mut self, scene: &mut SceneGraph) -> bool {
        let faces = self.mesh_face_selection();
        if faces.is_empty() {
            self.mesh_status = Some("Extrude needs a selected face (press 3 first).".into());
            return false;
        }
        self.mutate_mesh(scene, |mesh| mesh.extrude_faces(&faces, 0.25).map(|_| ()))
    }

    fn mesh_inset(&mut self, scene: &mut SceneGraph) -> bool {
        let faces = self.mesh_face_selection();
        if faces.is_empty() {
            self.mesh_status = Some("Inset needs a selected face (press 3 first).".into());
            return false;
        }
        self.mutate_mesh(scene, |mesh| mesh.inset_faces(&faces, 0.2).map(|_| ()))
    }

    fn mesh_subdivide(&mut self, scene: &mut SceneGraph) -> bool {
        let faces = self.mesh_face_selection();
        if faces.is_empty() {
            self.mesh_status = Some("Subdivide needs a selected face (press 3 first).".into());
            return false;
        }
        self.mutate_mesh(scene, |mesh| mesh.subdivide_faces(&faces).map(|_| ()))
    }

    fn mesh_delete_faces(&mut self, scene: &mut SceneGraph) -> bool {
        let faces = self.mesh_face_selection();
        if faces.is_empty() {
            self.mesh_status = Some("Delete needs a selected face (press 3 first).".into());
            return false;
        }
        let Some(id) = self.bridge.edit_session().node() else {
            return false;
        };
        self.begin_scene_edit(scene);
        let changed = scene
            .get_mut(id)
            .and_then(|node| node.editable_mesh.as_mut())
            .is_some_and(|mesh| {
                if mesh.delete_faces(&faces).is_err() {
                    return false;
                }
                mesh.touch();
                true
            });
        if changed {
            let emptied = scene
                .get(id)
                .and_then(|node| node.editable_mesh.as_ref())
                .is_none_or(|mesh| mesh.is_empty());
            if emptied {
                if let Some(node) = scene.get_mut(id) {
                    node.editable_mesh = None;
                }
            }
            if emptied {
                self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
                self.finish_mesh_edit(scene, true);
                return true;
            }
            self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
            self.bridge.edit_session_mut().clear_selection();
            self.refresh_mesh_status(scene);
        } else {
            self.scene_edit_snapshot = None;
        }
        changed
    }

    /// Double right-click on a face inserts a movable point and splits that
    /// triangle into three. This is the low-friction way to add topology
    /// without leaving Tab Mode or opening a modeling application.
    fn mesh_poke_at(
        &mut self,
        scene: &mut SceneGraph,
        view_proj: &glam::Mat4,
        size: [f32; 2],
        pointer: [f32; 2],
    ) -> bool {
        let Some((face, point)) = self
            .bridge
            .pick_edit_face_point(scene, view_proj, size[0], size[1], pointer)
        else {
            self.mesh_status =
                Some("Right-click a face twice to add a point on its surface.".into());
            return false;
        };
        let Some(id) = self.bridge.edit_session().node() else {
            return false;
        };
        self.begin_scene_edit(scene);
        let inserted = scene
            .get_mut(id)
            .and_then(|node| node.editable_mesh.as_mut())
            .and_then(|mesh| mesh.poke_face(face, point).ok());
        if let Some(vertex) = inserted {
            self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
            self.bridge
                .edit_session_mut()
                .set_mode(MeshSelectMode::Vertex);
            self.bridge.edit_session_mut().selected_vertices = vec![vertex];
            self.refresh_mesh_status(scene);
            true
        } else {
            self.scene_edit_snapshot = None;
            self.mesh_status = Some("Could not add a point to that face.".into());
            false
        }
    }

    fn mutate_mesh(
        &mut self,
        scene: &mut SceneGraph,
        operation: impl FnOnce(&mut raf_core::scene::EditableMeshData) -> Result<(), String>,
    ) -> bool {
        let Some(id) = self.bridge.edit_session().node() else {
            return false;
        };
        self.begin_scene_edit(scene);
        let changed = scene
            .get_mut(id)
            .and_then(|node| node.editable_mesh.as_mut())
            .is_some_and(|mesh| {
                if operation(mesh).is_err() {
                    return false;
                }
                mesh.touch();
                true
            });
        if changed {
            self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
            self.bridge.edit_session_mut().clear_selection();
            self.refresh_mesh_status(scene);
        } else {
            self.scene_edit_snapshot = None;
        }
        changed
    }

    fn update_camera(&mut self, input: ViewportInputFrame) -> bool {
        // Look-through is a fixed entity viewpoint; orbit/fly input must not
        // desync the rendered camera from the selection/manipulation camera.
        if self.look_through.is_some() {
            self.camera_navigation_active = false;
            return false;
        }
        let primary_tool_owns_pointer = input.button_down(PointerButton::Primary);
        let secondary = input.button_down(PointerButton::Secondary);
        let middle = input.button_down(PointerButton::Middle);
        let mut fly = input.fly_axis();
        if self.invert_ws {
            fly[2] = -fly[2];
        }
        if input.modifiers.command_modifier() {
            fly = [0.0; 3];
        }

        if input.pointer_enabled
            && input.modifiers.command_modifier()
            && input.scroll_delta[1].abs() > 0.01
        {
            self.wasd_speed_boost =
                (self.wasd_speed_boost - input.scroll_delta[1] * 0.002).clamp(0.5, 5.0);
        }

        let pointer_camera_active = !primary_tool_owns_pointer && (secondary || middle);
        let keyboard_camera_active = input.keyboard_enabled && fly != [0.0; 3];
        let scroll_camera_active =
            !input.modifiers.command_modifier() && input.scroll_delta != [0.0; 2];
        let input_active = pointer_camera_active || keyboard_camera_active || scroll_camera_active;
        if input_active {
            self.navigation_grace_seconds = 0.30;
        } else {
            self.navigation_grace_seconds =
                (self.navigation_grace_seconds - input.delta_seconds.max(1.0 / 240.0)).max(0.0);
        }
        let was_navigating = self.camera_navigation_active;
        self.camera_navigation_active = input_active || self.navigation_grace_seconds > 0.0;
        if self.camera_navigation_active != was_navigating {
            tracing::debug!(
                active = self.camera_navigation_active,
                "camera navigation state changed"
            );
        }
        self.bridge.handle_camera_input(
            ViewportPointerInput {
                pointer_delta: if pointer_camera_active {
                    input.pointer_delta
                } else {
                    [0.0; 2]
                },
                scroll_delta_y: if input.modifiers.command_modifier() {
                    0.0
                } else {
                    input.scroll_delta[1]
                },
                drag_secondary: !primary_tool_owns_pointer && secondary,
                drag_middle: !primary_tool_owns_pointer && middle,
                // WASD is tied to active editor context, not cursor hover.
                hovered: input.keyboard_enabled,
                move_forward: fly[2],
                move_right: fly[0],
                move_up: fly[1],
                frame_time_s: input.delta_seconds.max(1.0 / 240.0),
            },
            self.mode == NativeViewportMode::View2d,
            ViewportNavigationConfig {
                invert_mouse_x: self.invert_mouse_x,
                invert_mouse_y: self.invert_mouse_y,
                move_sensitivity: self.move_sensitivity,
                wasd_speed: self.wasd_speed * self.wasd_speed_boost,
                rotate_sensitivity: self.rotate_sensitivity,
                scale_sensitivity: self.scale_sensitivity,
            },
        );
        self.bridge
            .update_camera(self.mode == NativeViewportMode::View2d);
        was_navigating || pointer_camera_active || keyboard_camera_active || scroll_camera_active
    }

    fn update_primary_gesture(
        &mut self,
        input: ViewportInputFrame,
        scene: &mut SceneGraph,
        view_proj: &glam::Mat4,
        size: [f32; 2],
    ) -> bool {
        if !input.button_down(PointerButton::Primary) {
            return false;
        }
        let Some(pointer) = input.pointer_local else {
            return false;
        };
        if self.edit_mode == NativeViewportEditMode::Mesh {
            if !self.bridge.edit_session().is_dragging() {
                return false;
            }
            let changed = self.bridge.drag_selected_vertices(
                scene,
                self.move_sensitivity,
                pointer,
                self.mesh_axis,
            );
            if changed {
                self.refresh_mesh_status(scene);
            }
            return changed;
        }
        if self.bridge.active_drag_axis() != GizmoAxis::None {
            let multi = self
                .gesture_start
                .as_ref()
                .is_some_and(|gesture| gesture.movers.len() > 1);
            let changed = if multi {
                self.apply_group_axis_drag(
                    scene,
                    view_proj,
                    pointer,
                    input.modifiers.command_modifier(),
                    size,
                )
            } else {
                let moved = self.bridge.apply_transform_drag(
                    scene,
                    self.selected.first().copied(),
                    view_proj,
                    pointer,
                    self.uniform_scale_by_default,
                    input.modifiers.command_modifier(),
                    size[0],
                    size[1],
                );
                if moved {
                    if self.building_organized {
                        self.constrain_single_gizmo_drag(scene);
                    } else if self.snap_to_grid {
                        self.snap_single_gizmo_drag(scene);
                    }
                }
                moved
            };
            if changed {
                self.begin_scene_edit(scene);
            }
            return changed;
        }

        let Some(mut drag) = self.free_drag.take() else {
            return false;
        };
        let distance = ((pointer[0] - drag.pointer_start[0]).powi(2)
            + (pointer[1] - drag.pointer_start[1]).powi(2))
        .sqrt();
        if distance < 4.0 && !drag.moved {
            self.free_drag = Some(drag);
            return false;
        }
        let inverse = view_proj.inverse();
        let Some((start_origin, start_dir)) = raf_render::math::transform::screen_to_world_ray(
            drag.pointer_start[0],
            drag.pointer_start[1],
            size[0],
            size[1],
            &inverse,
        ) else {
            self.free_drag = Some(drag);
            return false;
        };
        let Some((current_origin, current_dir)) = raf_render::math::transform::screen_to_world_ray(
            pointer[0], pointer[1], size[0], size[1], &inverse,
        ) else {
            self.free_drag = Some(drag);
            return false;
        };
        let plane_normal = Vec3::Y;
        let start_t = raf_render::math::ray::ray_plane(
            &raf_render::math::ray::Ray::new(start_origin, start_dir),
            drag.plane_origin,
            plane_normal,
        )
        .unwrap_or(0.0);
        let current_t = raf_render::math::ray::ray_plane(
            &raf_render::math::ray::Ray::new(current_origin, current_dir),
            drag.plane_origin,
            plane_normal,
        )
        .unwrap_or(0.0);
        let delta =
            (current_origin + current_dir * current_t) - (start_origin + start_dir * start_t);
        let final_delta = if self.building_organized {
            let (resolved, clamped) = self.resolve_free_drag_delta(scene, &drag.members, delta);
            self.collision_contact |= clamped;
            resolved
        } else if self.snap_to_grid {
            Vec3::new(
                building_mode::snap_scalar(delta.x, self.grid_spacing),
                building_mode::snap_scalar(delta.y, self.grid_spacing),
                building_mode::snap_scalar(delta.z, self.grid_spacing),
            )
        } else {
            delta
        };
        for member in &drag.members {
            let parent = scene
                .get(member.id)
                .and_then(|node| node.parent)
                .map(|id| scene.world_matrix(id))
                .unwrap_or(Mat4::IDENTITY);
            if let Some(node) = scene.get_mut(member.id) {
                node.position =
                    member.local_start + parent.inverse().transform_vector3(final_delta);
            }
        }
        drag.moved = true;
        self.free_drag = Some(drag);
        true
    }

    fn apply_pick_selection(&mut self, picked: Option<SceneNodeId>, additive: bool) -> bool {
        let before = self.selected.clone();
        match (picked, additive) {
            (Some(id), true) => {
                if let Some(index) = self.selected.iter().position(|selected| *selected == id) {
                    self.selected.remove(index);
                } else {
                    self.selected.push(id);
                }
            }
            (Some(id), false) if !self.selected.contains(&id) => self.selected = vec![id],
            (Some(_), false) => {}
            (None, false) => self.selected.clear(),
            (None, true) => {}
        }
        before != self.selected
    }

    fn begin_free_drag(
        &mut self,
        scene: &SceneGraph,
        pointer: [f32; 2],
        picked: Option<SceneNodeId>,
    ) {
        let Some(picked) = picked else {
            self.free_drag = None;
            return;
        };
        let Some(node) = scene.get(picked).filter(|node| !node.locked) else {
            self.free_drag = None;
            return;
        };
        let members = if self.selected.len() > 1 && self.selected.contains(&picked) {
            self.selected
                .iter()
                .filter(|id| {
                    !self.selected.iter().any(|parent| {
                        *id != parent
                            && building_mode::is_descendant_of_any(scene, **id, &[*parent])
                    })
                })
                .filter_map(|id| {
                    scene
                        .get(*id)
                        .filter(|node| !node.locked)
                        .map(|node| FreeDragMember {
                            id: *id,
                            local_start: node.position,
                        })
                })
                .collect()
        } else {
            vec![FreeDragMember {
                id: picked,
                local_start: node.position,
            }]
        };
        if members.is_empty() {
            self.free_drag = None;
            return;
        }
        self.scene_edit_snapshot = Some(scene.clone());
        self.capture_gesture_start(scene);
        self.free_drag = Some(FreeDragGesture {
            pointer_start: pointer,
            plane_origin: scene.world_matrix(picked).col(3).truncate(),
            members,
            moved: false,
        });
    }

    fn begin_scene_edit(&mut self, scene: &SceneGraph) {
        if self.scene_edit_snapshot.is_none() {
            self.scene_edit_snapshot = Some(scene.clone());
        }
    }

    /// Gizmo anchor used for hit-testing and rendering. Single selections
    /// anchor on the entity; groups anchor on the centroid of their
    /// transformable members and expose the primary entity's scale so the
    /// scale handles keep a familiar size.
    fn gizmo_anchor(&self, scene: &SceneGraph) -> Option<(Vec3, Vec3)> {
        if self.edit_mode != NativeViewportEditMode::Object || self.selected.is_empty() {
            return None;
        }
        let single = self.selected.len() == 1;
        if !single && !self.multi_select_gizmo_enabled {
            return None;
        }
        let members = self.transform_member_ids(scene);
        if members.is_empty() {
            return None;
        }
        let mut count = 0.0f32;
        let mut sum = Vec3::ZERO;
        for id in &members {
            if scene.get(*id).is_some() {
                sum += scene.world_matrix(*id).transform_point3(Vec3::ZERO);
                count += 1.0;
            }
        }
        if count == 0.0 {
            return None;
        }
        let origin = if single { sum } else { sum / count };
        let scale_ref = scene
            .get(self.selected[0])
            .map(|node| node.scale)
            .unwrap_or(Vec3::ONE);
        Some((origin, scale_ref))
    }

    /// Selection members eligible for transform gestures. Groups drop locked
    /// entities; single selections preserve the historical behavior where the
    /// gizmo can still grab the one picked node.
    fn transform_member_ids(&self, scene: &SceneGraph) -> Vec<SceneNodeId> {
        if self.selected.len() <= 1 {
            return self.selected.clone();
        }
        self.selected
            .iter()
            .copied()
            .filter(|id| scene.get(*id).is_some_and(|node| !node.locked))
            .filter(|id| {
                !self.selected.iter().any(|parent| {
                    parent != id && building_mode::is_descendant_of_any(scene, *id, &[*parent])
                })
            })
            .collect()
    }

    /// Records pre-gesture transforms plus the starting union AABB so every
    /// later frame can rebuild final transforms deterministically.
    fn capture_gesture_start(&mut self, scene: &SceneGraph) {
        self.collision_contact = false;
        let ids = self.transform_member_ids(scene);
        let movers: Vec<GestureMover> = ids
            .iter()
            .filter_map(|id| {
                scene.get(*id).map(|node| GestureMover {
                    id: *id,
                    start_pos: node.position,
                    start_scale: node.scale,
                    start_rotation: node.rotation,
                })
            })
            .collect();
        if movers.is_empty() {
            self.gesture_start = None;
            return;
        }
        let pivot = movers
            .iter()
            .map(|m| scene.world_matrix(m.id).transform_point3(Vec3::ZERO))
            .sum::<Vec3>()
            / movers.len() as f32;
        let mut start_union: Option<Aabb> = None;
        for mover in &movers {
            if let Some(bounds) = building_mode::mover_aabb(scene, mover.id) {
                start_union = Some(match start_union {
                    Some(current) => current.unite(bounds),
                    None => bounds,
                });
            }
        }
        self.gesture_start = Some(GestureStart {
            movers,
            pivot,
            start_union,
        });
    }

    fn organized_step(&self) -> f32 {
        self.building_snap_step.max(1e-3)
    }

    fn gesture_active(&self) -> bool {
        self.bridge.active_drag_axis() != GizmoAxis::None || self.free_drag.is_some()
    }

    /// Applies one frame of gizmo drag to every group member using the
    /// renderer-resolved axis quantities, then runs organized constraints so
    /// the written transforms are already collision-free.
    fn apply_group_axis_drag(
        &mut self,
        scene: &mut SceneGraph,
        view_proj: &glam::Mat4,
        pointer: [f32; 2],
        snap_to_ctrl: bool,
        size: [f32; 2],
    ) -> bool {
        let Some(outcome) = self
            .bridge
            .compute_axis_drag(view_proj, pointer, size[0], size[1])
        else {
            return false;
        };
        let Some(gesture) = self.gesture_start.clone() else {
            return false;
        };
        let mode = self.bridge.gizmo().mode;
        let axis = match self.bridge.active_drag_axis() {
            GizmoAxis::X => 0usize,
            GizmoAxis::Y => 1,
            _ => 2,
        };
        let multi = gesture.movers.len() > 1;
        let step = self.organized_step();
        let walls = if self.building_organized {
            let ids: Vec<SceneNodeId> = gesture.movers.iter().map(|m| m.id).collect();
            building_mode::wall_aabbs(scene, &ids)
        } else {
            Vec::new()
        };

        match mode {
            GizmoMode::Translate => {
                let mut delta = outcome.axis_delta;
                if self.building_organized {
                    delta = building_mode::snap_scalar(delta, step);
                    if let Some(start_union) = gesture.start_union {
                        delta = building_mode::clamp_translation(start_union, &walls, axis, delta);
                    }
                } else if snap_to_ctrl || self.snap_to_grid {
                    let step = if self.snap_to_grid {
                        self.grid_spacing
                    } else {
                        CTRL_TRANSLATE_SNAP
                    };
                    delta = building_mode::snap_scalar(delta, step);
                }
                let offset = axis_direction(axis) * delta;
                for mover in &gesture.movers {
                    let parent = scene
                        .get(mover.id)
                        .and_then(|n| n.parent)
                        .map(|id| scene.world_matrix(id))
                        .unwrap_or(Mat4::IDENTITY);
                    if let Some(node) = scene.get_mut(mover.id) {
                        node.position =
                            mover.start_pos + parent.inverse().transform_vector3(offset);
                    }
                }
            }
            GizmoMode::Rotate => {
                let mut theta = outcome.rotation_radians;
                if self.building_organized || snap_to_ctrl {
                    theta = building_mode::snap_scalar(theta, ROTATE_SNAP_STEP);
                }
                if self.building_organized {
                    if let Some(start_union) = gesture.start_union {
                        theta = building_mode::clamp_rotation(
                            start_union,
                            gesture.pivot,
                            axis,
                            theta,
                            &walls,
                        );
                    }
                }
                let rotation = rotation_quat(axis, theta);
                for mover in &gesture.movers {
                    let parent = scene
                        .get(mover.id)
                        .and_then(|n| n.parent)
                        .map(|id| scene.world_matrix(id))
                        .unwrap_or(Mat4::IDENTITY);
                    let start = Quat::from_euler(
                        glam::EulerRot::YXZ,
                        mover.start_rotation.y.to_radians(),
                        mover.start_rotation.x.to_radians(),
                        mover.start_rotation.z.to_radians(),
                    );
                    let desired = Mat4::from_translation(gesture.pivot)
                        * Mat4::from_quat(rotation)
                        * Mat4::from_translation(-gesture.pivot)
                        * parent
                        * Mat4::from_scale_rotation_translation(
                            mover.start_scale,
                            start,
                            mover.start_pos,
                        );
                    let (scale, orientation, position) =
                        (parent.inverse() * desired).to_scale_rotation_translation();
                    let (y, x, z) = orientation.to_euler(glam::EulerRot::YXZ);
                    if let Some(node) = scene.get_mut(mover.id) {
                        node.position = position;
                        node.rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
                        node.scale = scale;
                    }
                }
            }
            GizmoMode::Scale => {
                let mut delta = outcome.axis_delta;
                if self.building_organized {
                    delta = building_mode::snap_scalar(delta, step);
                    if let Some(start_union) = gesture.start_union {
                        // Groups scale symmetrically around their centers;
                        // face compensation is a single-entity behavior.
                        let (allowed_min, allowed_max) = building_mode::clamp_growth(
                            start_union,
                            &walls,
                            axis,
                            -delta * 0.5,
                            delta * 0.5,
                        );
                        delta = (allowed_max - allowed_min) * delta.signum();
                    }
                } else if snap_to_ctrl {
                    delta = building_mode::snap_scalar(delta, CTRL_SCALE_SNAP);
                }
                let primary = match gesture.movers.first() {
                    Some(mover) => mover,
                    None => return false,
                };
                let start_axis = axis_of(primary.start_scale, axis).abs().max(0.01);
                let new_axis = (start_axis + delta).max(0.01);
                let uniform = self.uniform_scale_by_default;
                let factor = (new_axis / start_axis).max(0.05);
                for mover in &gesture.movers {
                    if let Some(node) = scene.get_mut(mover.id) {
                        if uniform {
                            node.scale = (mover.start_scale * factor).max(Vec3::splat(0.01));
                        } else {
                            let member_axis = axis_of(mover.start_scale, axis).abs().max(0.01);
                            let member_new = (member_axis + delta).max(0.01);
                            let mut next = mover.start_scale;
                            set_axis_of(&mut next, axis, member_new);
                            node.scale = next.max(Vec3::splat(0.01));
                        }
                    }
                }
                if uniform && multi {
                    for mover in &gesture.movers {
                        let parent = scene
                            .get(mover.id)
                            .and_then(|n| n.parent)
                            .map(|id| scene.world_matrix(id))
                            .unwrap_or(Mat4::IDENTITY);
                        let world = parent.transform_point3(mover.start_pos);
                        if let Some(node) = scene.get_mut(mover.id) {
                            node.position = parent
                                .inverse()
                                .transform_point3(gesture.pivot + (world - gesture.pivot) * factor);
                        }
                    }
                }
            }
        }
        true
    }

    /// Organized-style resolution for a single-entity gizmo drag: reads the
    /// desired parameter the bridge just wrote, snaps it to the calibrated
    /// step, clamps it against walls and rewrites the node flush with the
    /// blocking surface.
    fn constrain_single_gizmo_drag(&mut self, scene: &mut SceneGraph) {
        if !self.building_organized {
            return;
        }
        let mode = self.bridge.gizmo().mode;
        let axis = self.bridge.active_drag_axis();
        self.constrain_axis_drag(scene, mode, axis);
    }

    fn snap_single_gizmo_drag(&self, scene: &mut SceneGraph) {
        let Some(gesture) = self.gesture_start.as_ref() else {
            return;
        };
        let Some(mover) = gesture.movers.first() else {
            return;
        };
        let axis = match self.bridge.active_drag_axis() {
            GizmoAxis::X => 0usize,
            GizmoAxis::Y => 1usize,
            GizmoAxis::Z => 2usize,
            GizmoAxis::None => return,
        };
        let parent = scene
            .get(mover.id)
            .and_then(|node| node.parent)
            .map(|id| scene.world_matrix(id))
            .unwrap_or(Mat4::IDENTITY);
        let Some(node) = scene.get_mut(mover.id) else {
            return;
        };
        match self.bridge.gizmo().mode {
            GizmoMode::Translate => {
                let raw = axis_of(
                    parent.transform_vector3(node.position - mover.start_pos),
                    axis,
                );
                let snapped = building_mode::snap_scalar(raw, self.grid_spacing);
                node.position = mover.start_pos
                    + parent
                        .inverse()
                        .transform_vector3(axis_direction(axis) * snapped);
            }
            GizmoMode::Rotate | GizmoMode::Scale => {}
        }
    }

    /// Constraint core shared by the live gesture path and tests. `mode` and
    /// `axis` describe the active gizmo drag; gesture state must already be
    /// captured.
    fn constrain_axis_drag(&mut self, scene: &mut SceneGraph, mode: GizmoMode, axis: GizmoAxis) {
        let Some(gesture) = self.gesture_start.clone() else {
            return;
        };
        let Some(mover) = gesture.movers.first().copied() else {
            return;
        };
        let axis_index = match axis {
            GizmoAxis::X => 0usize,
            GizmoAxis::Y => 1,
            _ => 2,
        };
        let walls = building_mode::wall_aabbs(scene, &[mover.id]);
        let Some(node) = scene.get(mover.id) else {
            return;
        };
        let step = self.organized_step();
        let start_union = gesture.start_union.unwrap_or_else(unbounded_aabb);
        match mode {
            GizmoMode::Translate => {
                let parent = node
                    .parent
                    .map(|id| scene.world_matrix(id))
                    .unwrap_or(Mat4::IDENTITY);
                let raw_desired = axis_of(
                    parent.transform_vector3(node.position - mover.start_pos),
                    axis_index,
                );
                // Quantize first so decimal pointer motion becomes fixed
                // meter steps, then stop flush against the closest wall.
                let snapped = building_mode::snap_scalar(raw_desired, step);
                let clamped =
                    building_mode::clamp_translation(start_union, &walls, axis_index, snapped);
                mark_contact(&mut self.collision_contact, clamped, snapped);
                if let Some(node) = scene.get_mut(mover.id) {
                    node.position = mover.start_pos
                        + parent
                            .inverse()
                            .transform_vector3(axis_direction(axis_index) * clamped);
                }
            }
            GizmoMode::Rotate => {
                let raw_degrees =
                    axis_of(node.rotation, axis_index) - axis_of(mover.start_rotation, axis_index);
                let snapped =
                    building_mode::snap_scalar(raw_degrees.to_radians(), ROTATE_SNAP_STEP);
                let clamped = building_mode::clamp_rotation(
                    start_union,
                    gesture.pivot,
                    axis_index,
                    snapped,
                    &walls,
                );
                mark_contact(&mut self.collision_contact, clamped, snapped);
                if let Some(node) = scene.get_mut(mover.id) {
                    set_axis_of(
                        &mut node.rotation,
                        axis_index,
                        axis_of(mover.start_rotation, axis_index) + clamped.to_degrees(),
                    );
                }
            }
            GizmoMode::Scale => {
                let primitive = node.primitive;
                let raw_scale = node.scale;
                let parent = node
                    .parent
                    .map(|id| scene.world_matrix(id))
                    .unwrap_or(Mat4::IDENTITY);
                let rotation = Quat::from_euler(
                    glam::EulerRot::YXZ,
                    mover.start_rotation.y.to_radians(),
                    mover.start_rotation.x.to_radians(),
                    mover.start_rotation.z.to_radians(),
                );
                let local_axis = rotation * axis_direction(axis_index);
                let unit = axis_of(primitive.descriptor().half_extents, axis_index) * 2.0;
                let meters_per_scale = unit * parent.transform_vector3(local_axis).length();
                if meters_per_scale <= 1e-6 {
                    return;
                }
                let raw = axis_of(raw_scale - mover.start_scale, axis_index);
                let delta =
                    building_mode::snap_scalar(raw * meters_per_scale, step) / meters_per_scale;
                let old = axis_of(mover.start_scale, axis_index);
                let next_axis = old.signum() * (old.abs() + delta * old.signum()).max(0.01);
                let mut desired = mover.start_scale;
                if self.uniform_scale_by_default {
                    desired *= next_axis / old;
                } else {
                    set_axis_of(&mut desired, axis_index, next_axis);
                }
                let sign = self.drag_face_sign();
                let initial_overlaps = walls
                    .iter()
                    .filter(|wall| building_mode::penetrates_one(start_union, wall))
                    .copied()
                    .collect::<Vec<_>>();
                let blockers = walls
                    .iter()
                    .filter(|wall| !initial_overlaps.contains(wall))
                    .copied()
                    .collect::<Vec<_>>();
                let mut lo = 0.0f32;
                let mut hi = 1.0f32;
                let mut amount = 1.0;
                let mut collision = false;
                for _ in 0..18 {
                    let scale = mover.start_scale + (desired - mover.start_scale) * amount;
                    if let Some(node) = scene.get_mut(mover.id) {
                        node.scale = scale;
                        node.position = if self.uniform_scale_by_default {
                            mover.start_pos
                        } else {
                            mover.start_pos
                                + local_axis
                                    * (axis_of(scale - mover.start_scale, axis_index)
                                        * unit
                                        * 0.5
                                        * sign)
                        };
                    }
                    let blocked = building_mode::mover_aabb(scene, mover.id)
                        .is_some_and(|bounds| building_mode::penetrating(bounds, &blockers));
                    if !blocked && amount == 1.0 {
                        break;
                    }
                    if blocked {
                        hi = amount;
                        collision = true;
                    } else {
                        lo = amount;
                    }
                    amount = (lo + hi) * 0.5;
                }
                let scale = mover.start_scale
                    + (desired - mover.start_scale) * if collision { lo } else { 1.0 };
                if let Some(node) = scene.get_mut(mover.id) {
                    node.scale = scale;
                    node.position = if self.uniform_scale_by_default {
                        mover.start_pos
                    } else {
                        mover.start_pos
                            + local_axis
                                * (axis_of(scale - mover.start_scale, axis_index)
                                    * unit
                                    * 0.5
                                    * sign)
                    };
                }
                self.collision_contact = collision;
            }
        }
    }

    fn drag_face_sign(&self) -> f32 {
        let sign = self.bridge.highlighted_gizmo_scale_sign();
        if sign == 0.0 {
            1.0
        } else {
            sign.signum()
        }
    }

    /// Snaps and collision-clamps a free plane drag per axis so organized
    /// building stays on-grid and never enters walls.
    fn resolve_free_drag_delta(
        &self,
        scene: &SceneGraph,
        members: &[FreeDragMember],
        desired: Vec3,
    ) -> (Vec3, bool) {
        let step = self.organized_step();
        let mut resolved = Vec3::new(
            building_mode::snap_scalar(desired.x, step),
            building_mode::snap_scalar(desired.y, step),
            building_mode::snap_scalar(desired.z, step),
        );
        let Some(gesture) = self.gesture_start.as_ref() else {
            return (resolved, false);
        };
        let Some(mut remaining) = gesture.start_union else {
            return (resolved, false);
        };
        let ids: Vec<SceneNodeId> = members.iter().map(|member| member.id).collect();
        let walls = building_mode::wall_aabbs(scene, &ids);
        let mut clamped = false;
        for axis in 0..3 {
            let want = axis_of(resolved, axis);
            if want.abs() <= f32::EPSILON {
                continue;
            }
            let allowed = building_mode::clamp_translation(remaining, &walls, axis, want);
            if (allowed - want).abs() > 1e-5 {
                clamped = true;
            }
            set_axis_of(&mut resolved, axis, allowed);
            remaining = shift_aabb(remaining, axis, allowed);
        }
        (resolved, clamped)
    }

    fn render_options(&self) -> RenderOptions {
        let organized_gesture_grid = self.building_organized
            && self.gesture_active()
            && self.mode == NativeViewportMode::View3d;
        RenderOptions {
            environment_override: self
                .lighting_preview_neutral
                .then_some(raf_core::environment::EnvironmentSettings::default()),
            show_grid_3d: (self.grid_visible || organized_gesture_grid)
                && self.mode == NativeViewportMode::View3d,
            grid_spacing: self.grid_spacing,
            grid_load_distance: self.grid_load_distance.max(0.0),
            solid_show_surface_edges: self.solid_show_surface_edges,
            solid_xray_mode: self.solid_xray_mode,
            solid_face_tonality: self.solid_face_tonality,
            selection_outline: self.render_config.selection_outline,
            selection_outline_color: self.render_config.selection_outline_color,
            secondary_selection_outline_color: [255, 120, 20, 180],
            primary_selected: self.selected.first().map(|id| id.0 as u64),
            grid_y: GRID_Y,
            grid_no_depth_test: false,
            world_streaming_enabled: self.world_stream_config.enabled,
            world_stream_region_size: self.world_stream_config.region_size,
            world_stream_load_radius: self.world_stream_config.load_radius,
            geometry_detail: self.geometry_detail,
            texture_quality: self.texture_quality,
            particles: ParticleRenderOptions {
                enabled: self.particle_preview.enabled,
                paused: self.particle_preview.paused,
                frozen: self.particle_preview.frozen,
                stopped: self.particle_preview.stopped,
                time_scale: self.particle_preview.time_scale.clamp(0.0, 8.0),
                quality: raf_core::scene::ParticleQuality::Epic,
                max_sprites: self.particle_preview.max_sprites,
                wind: self.particle_preview.wind,
            },
            particle_dt: self.last_frame_seconds,
            camera_navigation: self.camera_navigation_active,
            stable_scene_packet: self.camera_navigation_active || self.scene_packet_reuse,
            scene_packet_reuse: self.scene_packet_reuse,
            focus_node: if self.edit_mode == NativeViewportEditMode::Mesh {
                self.selected.first().map(|id| id.0 as u64)
            } else {
                None
            },
        }
    }

    /// Camera framing guides: 90% action-safe frame plus rule-of-thirds lines.
    fn append_safe_frame(&self, frame: &mut SceneRenderFrame) {
        let width = frame.width as f32;
        let height = frame.height as f32;
        if width <= 1.0 || height <= 1.0 {
            return;
        }
        let frame_color = [255, 255, 255, 92];
        let guide_color = [255, 255, 255, 42];
        let margin = 0.05;
        let left = width * margin;
        let right = width * (1.0 - margin);
        let top = height * margin;
        let bottom = height * (1.0 - margin);
        for (start, end) in [
            ([left, top], [right, top]),
            ([right, top], [right, bottom]),
            ([right, bottom], [left, bottom]),
            ([left, bottom], [left, top]),
        ] {
            push_screen_line(frame, start, end, frame_color, 1.0);
        }
        for step in 1..3 {
            let x = width * (step as f32 / 3.0);
            let y = height * (step as f32 / 3.0);
            push_screen_line(frame, [x, 0.0], [x, height], guide_color, 1.0);
            push_screen_line(frame, [0.0, y], [width, y], guide_color, 1.0);
        }
    }

    /// Tab Mode overlay: selected faces, wireframe and draggable vertex
    /// handles, all in target pixels so they stay readable at any distance.
    fn append_mesh_edit_overlay(&self, scene: &SceneGraph, frame: &mut SceneRenderFrame) {
        if self.edit_mode != NativeViewportEditMode::Mesh {
            return;
        }
        let Some(overlay) = self.bridge.project_edit_overlay(
            scene,
            self.selected.first().copied(),
            &frame.view_proj,
            frame.width as f32,
            frame.height as f32,
        ) else {
            return;
        };
        for face in &overlay.faces {
            push_screen_triangle(frame, face.points, [255, 138, 24, 54]);
        }
        if overlay.edges.len() <= 4096 {
            for edge in &overlay.edges {
                let color = if edge.selected {
                    [255, 196, 92, 230]
                } else {
                    [255, 146, 32, 150]
                };
                push_screen_line(frame, edge.start, edge.end, color, 1.2);
            }
        }
        for vertex in &overlay.vertices {
            let fill = if vertex.selected {
                [255, 255, 255, 255]
            } else if vertex.hovered {
                [255, 216, 120, 255]
            } else {
                [255, 146, 32, 255]
            };
            push_screen_square(frame, vertex.position, 5.0, [26, 18, 10, 220]);
            push_screen_square(frame, vertex.position, 3.4, fill);
        }
    }

    fn append_renderer_owned_gizmo(&self, scene: &SceneGraph, frame: &mut SceneRenderFrame) {
        if self.edit_mode != NativeViewportEditMode::Object
            || !self.bridge.gizmo().visible
            || self.selected.is_empty()
        {
            return;
        }
        if self.selected.len() > 1 {
            if !self.multi_select_gizmo_enabled {
                return;
            }
            if let Some((origin, scale_ref)) = self.gizmo_anchor(scene) {
                GizmoRenderSpec {
                    mode: self.bridge.gizmo().mode,
                    active_axis: self.bridge.highlighted_gizmo_axis(),
                    active_scale_sign: self.bridge.highlighted_gizmo_scale_sign(),
                    origin,
                    entity_scale: scale_ref,
                    entity_axes: [Vec3::X, Vec3::Y, Vec3::Z],
                    enabled_scale_axes: [true; 3],
                    presentation_scale: self.gizmo_presentation_scale(
                        origin,
                        &frame.view_proj,
                        [frame.width as f32, frame.height as f32],
                    ),
                }
                .append_to(frame);
            }
            return;
        }
        let id = self.selected[0];
        let Some(node) = scene.get(id) else {
            return;
        };
        let world = scene.world_matrix(id);
        let (entity_axes, entity_scale) = raf_render::picking::gizmo_scale_basis(world);
        GizmoRenderSpec {
            mode: self.bridge.gizmo().mode,
            active_axis: self.bridge.highlighted_gizmo_axis(),
            active_scale_sign: self.bridge.highlighted_gizmo_scale_sign(),
            origin: world.col(3).truncate(),
            entity_scale,
            entity_axes,
            enabled_scale_axes: if node.primitive == raf_core::scene::graph::Primitive::Plane {
                [true, false, true]
            } else {
                [true; 3]
            },
            presentation_scale: self.gizmo_presentation_scale(
                world.col(3).truncate(),
                &frame.view_proj,
                [frame.width as f32, frame.height as f32],
            ),
        }
        .append_to(frame);
    }

    /// Organized-building feedback: while a gesture presses against walls the
    /// moving selection gets an orange contact outline so the flush limit is
    /// visible instead of silent.
    fn append_collision_contact_outline(&self, scene: &SceneGraph, frame: &mut SceneRenderFrame) {
        if !self.building_organized || !self.collision_contact || !self.gesture_active() {
            return;
        }
        let Some(gesture) = self.gesture_start.as_ref() else {
            return;
        };
        let mut union: Option<Aabb> = None;
        for mover in &gesture.movers {
            if let Some(bounds) = building_mode::mover_aabb(scene, mover.id) {
                union = Some(match union {
                    Some(current) => current.unite(bounds),
                    None => bounds,
                });
            }
        }
        let Some(bounds) = union else {
            return;
        };
        const CONTACT_COLOR: [u8; 4] = [255, 96, 32, 255];
        let corners = [
            Vec3::new(bounds.min.x, bounds.min.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.min.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.min.y, bounds.max.z),
            Vec3::new(bounds.min.x, bounds.min.y, bounds.max.z),
            Vec3::new(bounds.min.x, bounds.max.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.max.y, bounds.min.z),
            Vec3::new(bounds.max.x, bounds.max.y, bounds.max.z),
            Vec3::new(bounds.min.x, bounds.max.y, bounds.max.z),
        ];
        let edges = [
            (0, 1),
            (1, 2),
            (2, 3),
            (3, 0),
            (4, 5),
            (5, 6),
            (6, 7),
            (7, 4),
            (0, 4),
            (1, 5),
            (2, 6),
            (3, 7),
        ];
        for (start, end) in edges {
            frame.commands.draw_line(
                corners[start],
                corners[end],
                CONTACT_COLOR,
                1.6,
                true,
                -0.98,
            );
        }
    }

    /// World scale that lands the gizmo on a constant on-screen size.
    ///
    /// The old formula compared the orbit distance against a hardcoded `5.0`
    /// reference, so it ignored the real distance to the selected object, the
    /// viewport size and the projection. Measuring pixels-per-world-unit
    /// through the real matrix keeps arrows, rings and their hit geometry
    /// identical in size at any distance.
    /// World point the gizmo is attached to, or `None` when it is not shown.
    fn gizmo_origin(&self, scene: &SceneGraph) -> Option<Vec3> {
        if self.edit_mode != NativeViewportEditMode::Object || self.selected.is_empty() {
            return None;
        }
        if self.selected.len() == 1 {
            return scene.get(self.selected[0]).map(|node| node.position);
        }
        if !self.multi_select_gizmo_enabled {
            return None;
        }
        self.gizmo_anchor(scene).map(|(origin, _)| origin)
    }

    /// [`Self::gizmo_presentation_scale`] resolved for the current selection.
    fn gizmo_scale_for_scene(&self, scene: &SceneGraph, view_proj: &Mat4, size: [f32; 2]) -> f32 {
        match self.gizmo_origin(scene) {
            Some(origin) => self.gizmo_presentation_scale(origin, view_proj, size),
            None => 1.0,
        }
    }

    fn gizmo_presentation_scale(&self, origin: Vec3, view_proj: &Mat4, size: [f32; 2]) -> f32 {
        let (base_world, target_pixels) = match self.bridge.gizmo().mode {
            GizmoMode::Rotate => (
                raf_render::picking::GIZMO_ROTATION_RADIUS,
                raf_render::picking::GIZMO_TARGET_ROTATE_PIXELS,
            ),
            GizmoMode::Translate => (
                raf_render::picking::GIZMO_LENGTH,
                raf_render::picking::GIZMO_TARGET_TRANSLATE_PIXELS,
            ),
            // Scale handles are positioned by the object itself and their
            // radius is already screen-space, so there is nothing to scale.
            GizmoMode::Scale => return 1.0,
        };
        let pixels_per_unit =
            raf_render::picking::pixels_per_world_unit(view_proj, size[0], size[1], origin);
        if !pixels_per_unit.is_finite() || pixels_per_unit <= f32::EPSILON {
            return 1.0;
        }
        // `gizmo_growth_scale` is the user size multiplier on top of the
        // constant screen size: 0 keeps the standard size, 100 doubles it.
        let growth = 1.0 + self.gizmo_growth_scale.clamp(0.0, 100.0) / 100.0;
        let target = target_pixels * growth;
        (target / (base_world * pixels_per_unit)).clamp(0.02, 60.0)
    }
}

impl GameViewportPort for NativeGameViewportController {
    fn selected_ids(&self) -> Vec<SceneNodeId> {
        self.selected.clone()
    }

    fn set_selected_ids(&mut self, ids: Vec<SceneNodeId>) {
        if self.edit_mode != NativeViewportEditMode::Mesh {
            self.selected = ids;
        }
    }

    fn focus_entity(&mut self, scene: &SceneGraph, id: Option<SceneNodeId>) {
        if let Some(id) = id {
            self.selected = vec![id];
        }
        self.focus_selection(scene);
    }

    fn set_look_through(&mut self, scene: &SceneGraph, id: Option<SceneNodeId>) {
        self.set_look_through(scene, id);
    }
}

fn axis_of(vector: Vec3, axis: usize) -> f32 {
    match axis {
        0 => vector.x,
        1 => vector.y,
        _ => vector.z,
    }
}

fn set_axis_of(vector: &mut Vec3, axis: usize, value: f32) {
    match axis {
        0 => vector.x = value,
        1 => vector.y = value,
        _ => vector.z = value,
    }
}

fn axis_direction(axis: usize) -> Vec3 {
    match axis {
        0 => Vec3::X,
        1 => Vec3::Y,
        _ => Vec3::Z,
    }
}

fn rotation_quat(axis: usize, theta: f32) -> Quat {
    match axis {
        0 => Quat::from_rotation_x(theta),
        1 => Quat::from_rotation_y(theta),
        _ => Quat::from_rotation_z(theta),
    }
}

fn shift_aabb(bounds: Aabb, axis: usize, delta: f32) -> Aabb {
    let mut bounds = bounds;
    match axis {
        0 => {
            bounds.min.x += delta;
            bounds.max.x += delta;
        }
        1 => {
            bounds.min.y += delta;
            bounds.max.y += delta;
        }
        _ => {
            bounds.min.z += delta;
            bounds.max.z += delta;
        }
    }
    bounds
}

/// Fallback mover bounds for gestures on geometry-less selections: large
/// enough that walls can never constrain them.
fn unbounded_aabb() -> Aabb {
    Aabb {
        min: Vec3::splat(-1.0e6),
        max: Vec3::splat(1.0e6),
    }
}

fn mark_contact(flag: &mut bool, clamped: f32, raw: f32) {
    if (clamped - raw).abs() > 1e-4 {
        *flag = true;
    }
}

fn push_screen_triangle(frame: &mut SceneRenderFrame, points: [[f32; 2]; 3], color: [u8; 4]) {
    frame
        .commands
        .draw_screen_triangle(BasicScreenTriangle { points, color });
}

fn push_screen_square(frame: &mut SceneRenderFrame, center: [f32; 2], half: f32, color: [u8; 4]) {
    let min = [center[0] - half, center[1] - half];
    let max = [center[0] + half, center[1] + half];
    push_screen_triangle(frame, [min, [max[0], min[1]], max], color);
    push_screen_triangle(frame, [min, max, [min[0], max[1]]], color);
}

fn push_screen_line(
    frame: &mut SceneRenderFrame,
    start: [f32; 2],
    end: [f32; 2],
    color: [u8; 4],
    width: f32,
) {
    let dx = end[0] - start[0];
    let dy = end[1] - start[1];
    let length = (dx * dx + dy * dy).sqrt();
    if !length.is_finite() || length < 0.5 {
        return;
    }
    let half = (width * 0.5).max(0.35);
    let nx = -dy / length * half;
    let ny = dx / length * half;
    let a = [start[0] + nx, start[1] + ny];
    let b = [end[0] + nx, end[1] + ny];
    let c = [end[0] - nx, end[1] - ny];
    let d = [start[0] - nx, start[1] - ny];
    push_screen_triangle(frame, [a, b, c], color);
    push_screen_triangle(frame, [a, c, d], color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use raf_core::scene::Primitive;

    fn scene_with_wall() -> (SceneGraph, SceneNodeId, SceneNodeId) {
        let mut scene = SceneGraph::default();
        let mover = scene.add_root_with_primitive("Mover", Primitive::Cube);
        let wall = scene.add_root_with_primitive("Wall", Primitive::Cube);
        if let Some(node) = scene.get_mut(wall) {
            node.position = Vec3::new(3.0, 0.0, 0.0);
        }
        (scene, mover, wall)
    }

    #[test]
    fn authoring_face_scaling_keeps_anchor_with_rotation_and_parent_scale() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Parent");
        scene.get_mut(parent).unwrap().scale = Vec3::new(2.0, 1.0, 3.0);
        scene.get_mut(parent).unwrap().rotation.y = 30.0;
        let id = scene.add_child_with_primitive(parent, "Capsule", Primitive::Capsule);
        scene.get_mut(id).unwrap().rotation.z = 90.0;
        let mut controller = organized_controller(id);
        controller.uniform_scale_by_default = false;
        controller.capture_gesture_start(&scene);
        let before = scene
            .world_matrix(id)
            .transform_point3(Vec3::new(0.0, -1.0, 0.0));
        scene.get_mut(id).unwrap().scale.y = 2.2;
        controller.constrain_axis_drag(&mut scene, GizmoMode::Scale, GizmoAxis::Y);
        let after = scene
            .world_matrix(id)
            .transform_point3(Vec3::new(0.0, -1.0, 0.0));
        assert!(
            (before - after).length() < 1e-4,
            "opposite face moved: {before:?} -> {after:?}"
        );
    }

    fn organized_controller(id: SceneNodeId) -> NativeGameViewportController {
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![id];
        controller.building_organized = true;
        controller.building_snap_step = 1.0;
        controller
    }

    #[test]
    fn organized_translate_snaps_to_full_meters() {
        let mut scene = SceneGraph::default();
        let mover = scene.add_root_with_primitive("Mover", Primitive::Cube);
        let mut controller = organized_controller(mover);
        controller.capture_gesture_start(&scene);
        if let Some(node) = scene.get_mut(mover) {
            node.position.x = 2.37;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Translate, GizmoAxis::X);
        assert!(
            (scene.get(mover).unwrap().position.x - 2.0).abs() < 1e-4,
            "2.37 m of pointer motion must land on the 2 m grid line"
        );
    }

    #[test]
    fn organized_translate_stops_flush_before_wall() {
        let (mut scene, mover, _wall) = scene_with_wall();
        let mut controller = organized_controller(mover);
        controller.capture_gesture_start(&scene);
        if let Some(node) = scene.get_mut(mover) {
            node.position.x = 7.4;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Translate, GizmoAxis::X);
        let x = scene.get(mover).unwrap().position.x;
        assert!(
            (x - 2.0).abs() < 1e-3,
            "mover must sit flush 1 m from the wall face, got {x}"
        );
        assert!(controller.collision_contact, "wall contact must be flagged");
    }

    #[test]
    fn organized_translate_clamps_on_every_axis() {
        for (axis, wall_offset, drag_axis) in [
            (0usize, Vec3::new(3.0, 0.0, 0.0), GizmoAxis::X),
            (1usize, Vec3::new(0.0, 3.0, 0.0), GizmoAxis::Y),
            (2usize, Vec3::new(0.0, 0.0, 3.0), GizmoAxis::Z),
        ] {
            let mut scene = SceneGraph::default();
            let mover = scene.add_root_with_primitive("Mover", Primitive::Cube);
            let wall = scene.add_root_with_primitive("Wall", Primitive::Cube);
            if let Some(node) = scene.get_mut(wall) {
                node.position = wall_offset;
            }
            let mut controller = organized_controller(mover);
            controller.capture_gesture_start(&scene);
            let start = axis_of(scene.get(mover).unwrap().position, axis);
            if let Some(node) = scene.get_mut(mover) {
                set_axis_of(&mut node.position, axis, start + 6.4);
            }
            controller.constrain_axis_drag(&mut scene, GizmoMode::Translate, drag_axis);
            let final_value = axis_of(scene.get(mover).unwrap().position, axis);
            assert!(
                (final_value - 2.0).abs() < 1e-3,
                "axis {axis} must stop flush before the wall, got {final_value}"
            );
            assert!(
                controller.collision_contact,
                "axis {axis} must flag wall contact"
            );
        }
    }

    #[test]
    fn organized_rotate_stays_free_without_live_walls() {
        let mut scene = SceneGraph::default();
        let mover = scene.add_root_with_primitive("Mover", Primitive::Cube);
        if let Some(node) = scene.get_mut(mover) {
            node.position = Vec3::new(6.0, 0.0, 0.0);
        }
        let wall = scene.add_root_with_primitive("Wall", Primitive::Cube);
        if let Some(node) = scene.get_mut(wall) {
            node.position = Vec3::new(0.0, 0.0, 0.0);
        }
        let mut controller = organized_controller(mover);
        controller.capture_gesture_start(&scene);
        if let Some(node) = scene.get_mut(mover) {
            node.rotation.y = 47.5;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Rotate, GizmoAxis::Y);
        let rotation = scene.get(mover).unwrap().rotation.y;
        assert!(
            (rotation - 45.0).abs() < 1e-3,
            "isolated entity must rotate to the 45-degree snap, got {rotation}"
        );
    }

    #[test]
    fn organized_rotate_ignores_already_swallowed_walls() {
        // Mover resting half-inside a big floor slab: rotation must stay
        // possible because the overlap predates the gesture.
        let mut scene = SceneGraph::default();
        let mover = scene.add_root_with_primitive("Mover", Primitive::Cube);
        let floor = scene.add_root_with_primitive("Floor", Primitive::Plane);
        if let Some(node) = scene.get_mut(floor) {
            node.scale = Vec3::new(40.0, 1.0, 40.0);
        }
        let mut controller = organized_controller(mover);
        controller.capture_gesture_start(&scene);
        if let Some(node) = scene.get_mut(mover) {
            node.rotation.y = 90.0;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Rotate, GizmoAxis::Y);
        let rotation = scene.get(mover).unwrap().rotation.y;
        assert!(
            (rotation - 90.0).abs() < 1e-3,
            "pre-swallowed floor must not lock rotation, got {rotation}"
        );
    }

    #[test]
    fn organized_scale_grows_in_meter_steps_and_stops_at_wall() {
        let (mut scene, mover, _wall) = scene_with_wall();
        let mut controller = organized_controller(mover);
        controller.capture_gesture_start(&scene);
        if let Some(node) = scene.get_mut(mover) {
            node.scale.x = 3.7;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Scale, GizmoAxis::X);
        let node = scene.get(mover).unwrap();
        // 3.7 snapped to 4.0 would reach x in [2, 6]; wall face at 2.5 caps
        // the +X face, so the allowed growth lands flush at 3.0 total size.
        assert!(
            (node.scale.x - 3.0).abs() < 1e-3,
            "scale must stop flush against the wall, got {}",
            node.scale.x
        );
    }

    #[test]
    fn engine_settings_sync_wasd_navigation_preferences() {
        let mut controller = NativeGameViewportController::default();
        let mut settings = EngineSettings::default();
        settings.wasd_speed = 2.75;
        settings.invert_ws = true;
        settings.move_gizmo_sensitivity = 1.25;

        controller.apply_engine_settings(&settings);

        assert!((controller.wasd_speed - 2.75).abs() < f32::EPSILON);
        assert!(controller.invert_ws);
        assert!((controller.move_sensitivity - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn project_geometry_detail_reaches_the_viewport_renderer_options() {
        let mut controller = NativeGameViewportController::default();
        let mut settings = ProjectSettings::default();
        settings.geometry_detail = GeometryDetailMode::Detailed;

        controller.apply_project_settings(&settings);

        assert_eq!(
            controller.render_options().geometry_detail,
            GeometryDetailMode::Detailed
        );
    }

    #[test]
    fn passive_cursor_motion_does_not_invalidate_the_viewport() {
        let mut controller = NativeGameViewportController::default();
        let mut router = InputRouter::default();
        let mut scene = SceneGraph::default();
        let mut input = InputSnapshot::default();
        input.pointer_position = Some([120.0, 80.0]);
        input.pointer_delta = [1.0, -1.0];

        let update = controller.process_input(
            &input,
            &mut router,
            ViewportInputRect::new(0.0, 0.0, 800.0, 600.0),
            &mut scene,
        );

        assert!(!update.camera_motion);
        assert!(!update.needs_redraw);
    }

    #[test]
    fn tab_mesh_mode_converts_and_cancel_restores_the_primitive() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![id];

        assert!(controller.enter_mesh_edit(&mut scene));
        assert_eq!(controller.edit_mode, NativeViewportEditMode::Mesh);
        assert!(scene.get(id).unwrap().editable_mesh.is_some());

        assert!(controller.finish_mesh_edit(&mut scene, false));
        assert_eq!(controller.edit_mode, NativeViewportEditMode::Object);
        assert!(scene.get(id).unwrap().editable_mesh.is_none());
    }

    #[test]
    fn tab_mesh_mode_apply_keeps_the_authored_topology() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![id];

        assert!(controller.enter_mesh_edit(&mut scene));
        controller.bridge.edit_session_mut().selected_faces = vec![0];
        assert!(controller.mesh_extrude(&mut scene));
        let faces = scene
            .get(id)
            .and_then(|node| node.editable_mesh.as_ref())
            .map(|mesh| mesh.face_count())
            .unwrap_or_default();
        assert!(faces > 12);

        assert!(!controller.finish_mesh_edit(&mut scene, true));
        assert_eq!(controller.edit_mode, NativeViewportEditMode::Object);
        assert_eq!(
            scene
                .get(id)
                .and_then(|node| node.editable_mesh.as_ref())
                .map(|mesh| mesh.face_count()),
            Some(faces)
        );
    }

    #[test]
    fn double_right_click_adds_a_movable_point_on_the_clicked_face() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![id];
        assert!(controller.enter_mesh_edit(&mut scene));
        let view_proj = controller.bridge().view_projection(800.0, 600.0);
        let world = scene
            .world_matrix(id)
            .transform_point3(Vec3::new(0.0, 0.0, 0.5));
        let (pointer, _) =
            raf_render::math::transform::project_point(world, &view_proj, 800.0, 600.0)
                .expect("face projects on screen");
        let before = scene.get(id).unwrap().editable_mesh.as_ref().unwrap();
        let (faces, vertices) = (before.face_count(), before.vertices.len());
        assert!(controller.mesh_poke_at(&mut scene, &view_proj, [800.0, 600.0], pointer));
        let after = scene.get(id).unwrap().editable_mesh.as_ref().unwrap();
        assert_eq!(after.face_count(), faces + 2);
        assert_eq!(after.vertices.len(), vertices + 1);
        assert_eq!(
            controller.bridge.edit_session().selected_vertices.len(),
            1,
            "the inserted point becomes the active selection"
        );
    }

    #[test]
    fn camera_navigation_reuses_the_scene_packet_with_many_objects() {
        let mut scene = SceneGraph::default();
        for index in 0..24 {
            let id = scene.add_root_with_primitive(&format!("Cube {index}"), Primitive::Cube);
            if let Some(node) = scene.get_mut(id) {
                let angle = index as f32 / 24.0 * std::f32::consts::TAU;
                node.position = Vec3::new(angle.cos() * 6.0, index as f32 * 0.1, angle.sin() * 6.0);
            }
        }
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![scene.iter().next().unwrap().0];
        controller.bridge.gizmo_mut().visible = true;
        controller.camera_navigation_active = true;

        let first = controller.build_scene_frame(&scene, [800, 600]);
        let first_hash = first.commands.content_hash();
        controller.bridge.set_orbit_angles(0.4, 0.2);
        controller.bridge.update_camera(false);
        let second = controller.build_scene_frame(&scene, [800, 600]);
        let second_hash = second.commands.content_hash();
        assert_eq!(
            first_hash, second_hash,
            "camera navigation must not invalidate the retained scene packet"
        );
    }

    #[test]
    fn camera_navigation_keeps_the_scene_packet_content_stable() {
        let mut scene = SceneGraph::default();
        let id = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let mut controller = NativeGameViewportController::default();
        controller.selected = vec![id];
        controller.bridge.gizmo_mut().visible = true;
        controller.camera_navigation_active = true;

        let first = controller.build_scene_frame(&scene, [800, 600]);
        let first_hash = first.commands.content_hash();
        controller.bridge.set_orbit_angles(0.4, 0.2);
        controller.bridge.update_camera(false);
        let second = controller.build_scene_frame(&scene, [800, 600]);
        let second_hash = second.commands.content_hash();
        assert_eq!(
            first_hash, second_hash,
            "camera navigation must not invalidate the retained scene packet"
        );
    }

    #[test]
    fn look_through_rejects_nodes_without_a_game_camera() {
        let mut scene = SceneGraph::default();
        let plain = scene.add_root_with_primitive("Plain", Primitive::Cube);
        let mut controller = NativeGameViewportController::default();

        controller.set_look_through(&scene, Some(plain));
        assert!(!controller.look_through_active());

        if let Some(node) = scene.get_mut(plain) {
            node.game_camera = Some(raf_core::runtime_config::GameCamera::default());
        }
        controller.set_look_through(&scene, Some(plain));
        assert!(controller.look_through_active());
        assert!(controller.look_through_camera(&scene).is_some());
    }
}
