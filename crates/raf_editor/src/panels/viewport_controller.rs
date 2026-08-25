//! Native, backend-neutral Game viewport controller.
//!
//! This is the native interaction path for the editor viewport.
//! It owns camera/tool/selection gestures and records a renderer-owned gizmo;
//! window placement, RafUI chrome, and final presentation stay outside.

use glam::{Quat, Vec3};
use raf_core::config::EngineSettings;
use raf_core::scene::{SceneGraph, SceneNodeId};
use raf_core::{InputKey, InputOwner, InputRouter, InputSnapshot, PointerButton};
use raf_render::api_graphic_basic::device::SceneFrameOutput;
use raf_render::bridge::{
    try_capture_camera, try_capture_gizmo, try_capture_viewport_tool, GizmoRenderSpec,
    RenderRuntime, ViewportBridge, ViewportInputFrame, ViewportInputRect, ViewportNavigationConfig,
    ViewportPointerInput,
};
use raf_render::gizmo::{GizmoAxis, GizmoMode};
use raf_render::render_config::RenderConfig;
use raf_render::scene_renderer::{RenderMode, RenderOptions, SceneRenderFrame, GRID_Y};
use raf_render::WorldStreamConfig;

use crate::building_mode::{self, Aabb};
use crate::commands::game::GameViewportPort;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeViewportRenderStyle {
    #[default]
    Solid,
    Wireframe,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NativeViewportEditMode {
    #[default]
    Object,
    Vertex,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct NativeViewportUpdate {
    pub scene_changed: bool,
    pub selection_changed: bool,
    pub gesture_finished: bool,
    pub needs_redraw: bool,
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
    pub selected: Vec<SceneNodeId>,
    pub grid_visible: bool,
    pub grid_spacing: f32,
    pub grid_load_distance: f32,
    pub show_labels: bool,
    pub solid_show_surface_edges: bool,
    pub solid_xray_mode: bool,
    pub solid_face_tonality: bool,
    pub invert_mouse_x: bool,
    pub invert_mouse_y: bool,
    pub invert_ws: bool,
    pub move_sensitivity: f32,
    pub rotate_sensitivity: f32,
    pub scale_sensitivity: f32,
    pub wasd_speed: f32,
    pub wasd_speed_boost: f32,
    pub uniform_scale_by_default: bool,
    /// Engine setting: render and honor the gizmo while several entities are
    /// selected. When false the gizmo stays single-selection only.
    pub multi_select_gizmo_enabled: bool,
    /// Project building style: quantized, collision-aware authoring.
    pub building_organized: bool,
    /// Fixed metric quantum used by the organized style (meters).
    pub building_snap_step: f32,
    render_config: RenderConfig,
    world_stream_config: WorldStreamConfig,
    bridge: ViewportBridge,
    free_drag: Option<FreeDragGesture>,
    scene_edit_snapshot: Option<SceneGraph>,
    completed_scene_edit_snapshot: Option<SceneGraph>,
    viewport_active: bool,
    gesture_start: Option<GestureStart>,
    /// True when the latest organized-resolution frame had to back the
    /// gesture off against a wall. Drives the contact outline overlay.
    collision_contact: bool,
}

impl Default for NativeGameViewportController {
    fn default() -> Self {
        Self {
            mode: NativeViewportMode::View3d,
            render_style: NativeViewportRenderStyle::Solid,
            edit_mode: NativeViewportEditMode::Object,
            selected: Vec::new(),
            grid_visible: true,
            grid_spacing: 1.0,
            grid_load_distance: 15.0,
            show_labels: true,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,
            invert_mouse_x: false,
            invert_mouse_y: true,
            invert_ws: false,
            move_sensitivity: 3.5,
            rotate_sensitivity: 3.5,
            scale_sensitivity: 3.5,
            wasd_speed: 1.0,
            wasd_speed_boost: 1.0,
            uniform_scale_by_default: false,
            multi_select_gizmo_enabled: true,
            building_organized: false,
            building_snap_step: 1.0,
            render_config: RenderConfig::default(),
            world_stream_config: WorldStreamConfig::default(),
            bridge: ViewportBridge::default(),
            free_drag: None,
            scene_edit_snapshot: None,
            completed_scene_edit_snapshot: None,
            viewport_active: true,
            gesture_start: None,
            collision_contact: false,
        }
    }
}

impl NativeGameViewportController {
    pub fn bridge(&self) -> &ViewportBridge {
        &self.bridge
    }

    pub fn bridge_mut(&mut self) -> &mut ViewportBridge {
        &mut self.bridge
    }

    pub fn set_active(&mut self, active: bool) {
        self.viewport_active = active;
    }

    /// Applies engine-wide viewport preferences to the live native controller.
    /// The settings surface owns the draft, while this controller owns the
    /// values consumed by camera and gizmo input, so they must be synchronized
    /// explicitly before processing a frame.
    pub fn apply_engine_settings(&mut self, settings: &EngineSettings) {
        self.invert_mouse_x = settings.invert_mouse_x;
        self.invert_mouse_y = settings.invert_mouse_y;
        self.invert_ws = settings.invert_ws;
        self.wasd_speed = settings.wasd_speed.clamp(0.05, 5.0);
        self.move_sensitivity = settings.move_gizmo_sensitivity.clamp(0.25, 4.0);
        self.rotate_sensitivity = settings.rotate_gizmo_sensitivity.clamp(0.25, 4.0);
        self.scale_sensitivity = settings.scale_gizmo_sensitivity.clamp(0.25, 4.0);
        self.uniform_scale_by_default = settings.uniform_scale_by_default;
        self.multi_select_gizmo_enabled = settings.multi_select_gizmo_enabled;
    }

    pub fn process_input(
        &mut self,
        input: &InputSnapshot,
        router: &mut InputRouter,
        rect: ViewportInputRect,
        scene: &mut SceneGraph,
    ) -> NativeViewportUpdate {
        self.selected.retain(|id| scene.is_valid_node(*id));
        let mut update = NativeViewportUpdate::default();
        let size = [rect.size[0].max(1.0), rect.size[1].max(1.0)];

        // Tool hotkeys are evaluated before camera movement, but never while
        // a primary gesture or command chord owns the frame. T is Scale; S is
        // permanently left to WASD navigation.
        let before_capture =
            ViewportInputFrame::from_snapshot(input, router, rect, self.viewport_active);
        self.apply_tool_shortcuts(before_capture, scene);

        self.bridge
            .set_picking_policy(raf_render::bridge::PickingPolicy::for_render_config(
                &self.render_config,
            ));
        self.bridge
            .update_camera(self.mode == NativeViewportMode::View2d);
        let view_proj = self.bridge.view_projection(size[0], size[1]);

        if self.edit_mode == NativeViewportEditMode::Object {
            if let Some(pointer) = before_capture.pointer_local {
                if let Some((origin, scale_ref)) = self.gizmo_anchor(scene) {
                    self.bridge.update_transform_hover_world(
                        origin,
                        scale_ref,
                        &view_proj,
                        pointer,
                        size[0],
                        size[1],
                        self.gizmo_presentation_scale(),
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
            }
        }

        if input.button_pressed(PointerButton::Secondary) {
            let _ = try_capture_camera(router, input, rect, PointerButton::Secondary);
        }
        if input.button_pressed(PointerButton::Middle) {
            let _ = try_capture_camera(router, input, rect, PointerButton::Middle);
        }

        if input.button_pressed(PointerButton::Primary) {
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
                            self.gizmo_presentation_scale(),
                        );
                    } else {
                        self.bridge.begin_transform_drag_scaled(
                            scene,
                            self.selected.first().copied(),
                            &view_proj,
                            pointer,
                            size[0],
                            size[1],
                            self.gizmo_presentation_scale(),
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
        update.scene_changed |= self.update_primary_gesture(routed, scene, &view_proj, size);
        update.needs_redraw |= self.update_camera(routed);

        if routed.button_released(PointerButton::Primary)
            || (!input.button_down(PointerButton::Primary)
                && matches!(
                    router.pointer_owner(PointerButton::Primary),
                    Some(InputOwner::ViewportGizmo | InputOwner::ViewportTool)
                ))
        {
            let changed = self.free_drag.as_ref().is_some_and(|drag| drag.moved)
                || self.bridge.active_drag_axis() != GizmoAxis::None;
            self.bridge.end_transform_drag();
            self.free_drag = None;
            self.gesture_start = None;
            self.collision_contact = false;
            if changed {
                self.completed_scene_edit_snapshot = self.scene_edit_snapshot.take();
                update.gesture_finished = true;
            } else {
                self.scene_edit_snapshot = None;
            }
        }

        update.needs_redraw |= update.scene_changed
            || update.selection_changed
            || routed.requires_continuous_redraw()
            || self.bridge.update_smooth_focus();
        update
    }

    pub fn build_scene_frame(&mut self, scene: &SceneGraph, size: [u32; 2]) -> SceneRenderFrame {
        let width = size[0].max(1);
        let height = size[1].max(1);
        self.bridge
            .update_camera(self.mode == NativeViewportMode::View2d);
        let light_dir = Vec3::new(0.4, 0.8, 0.6).normalize();
        let mut frame = self.bridge.build_scene_frame(
            scene,
            width as f32,
            height as f32,
            &self.selected,
            [240, 240, 242, 255],
            light_dir,
            self.render_options(),
            self.edit_mode == NativeViewportEditMode::Vertex,
        );
        self.append_renderer_owned_gizmo(scene, &mut frame);
        self.append_collision_contact_outline(scene, &mut frame);
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

    fn apply_tool_shortcuts(&mut self, input: ViewportInputFrame, scene: &SceneGraph) {
        if !input.keyboard_enabled
            || input.button_down(PointerButton::Primary)
            || input.modifiers.command_modifier()
        {
            return;
        }
        if input.key_pressed(InputKey::G) {
            self.set_gizmo_mode(GizmoMode::Translate);
        } else if input.key_pressed(InputKey::R) {
            self.set_gizmo_mode(GizmoMode::Rotate);
        } else if input.key_pressed(InputKey::T) {
            self.set_gizmo_mode(GizmoMode::Scale);
        } else if input.key_pressed(InputKey::C) {
            self.bridge.gizmo_mut().visible = false;
        } else if input.key_pressed(InputKey::F) {
            self.focus_selection(scene);
        } else if input.key_pressed(InputKey::Tab) && !self.selected.is_empty() {
            self.edit_mode = match self.edit_mode {
                NativeViewportEditMode::Object => NativeViewportEditMode::Vertex,
                NativeViewportEditMode::Vertex => NativeViewportEditMode::Object,
            };
            self.bridge.clear_edit_drag_state();
            if self.edit_mode == NativeViewportEditMode::Vertex {
                self.bridge
                    .prepare_selected_edit_mesh(scene, self.selected.first().copied());
            }
        }
        if input.key_pressed(InputKey::Numpad2) {
            self.mode = NativeViewportMode::View2d;
        } else if input.key_pressed(InputKey::Numpad3) {
            self.mode = NativeViewportMode::View3d;
        }
    }

    fn update_camera(&mut self, input: ViewportInputFrame) -> bool {
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
        pointer_camera_active || keyboard_camera_active || input.scroll_delta != [0.0; 2]
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
                    self.constrain_single_gizmo_drag(scene);
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
        } else {
            delta
        };
        for member in &drag.members {
            if let Some(node) = scene.get_mut(member.id) {
                node.position = member.local_start + final_delta;
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
            if let Some(node) = scene.get(*id) {
                sum += node.position;
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
        let pivot = if movers.len() == 1 {
            movers[0].start_pos
        } else {
            movers.iter().fold(Vec3::ZERO, |acc, m| acc + m.start_pos) / movers.len() as f32
        };
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
                } else if snap_to_ctrl {
                    delta = building_mode::snap_scalar(delta, CTRL_TRANSLATE_SNAP);
                }
                let offset = axis_direction(axis) * delta;
                for mover in &gesture.movers {
                    if let Some(node) = scene.get_mut(mover.id) {
                        node.position = mover.start_pos + offset;
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
                    if let Some(node) = scene.get_mut(mover.id) {
                        node.position =
                            gesture.pivot + rotation * (mover.start_pos - gesture.pivot);
                        set_axis_of(
                            &mut node.rotation,
                            axis,
                            axis_of(mover.start_rotation, axis) + theta,
                        );
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
                        if let Some(node) = scene.get_mut(mover.id) {
                            node.position =
                                gesture.pivot + (mover.start_pos - gesture.pivot) * factor;
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
                let raw_desired =
                    axis_of(node.position, axis_index) - axis_of(mover.start_pos, axis_index);
                // Quantize first so decimal pointer motion becomes fixed
                // meter steps, then stop flush against the closest wall.
                let snapped = building_mode::snap_scalar(raw_desired, step);
                let clamped =
                    building_mode::clamp_translation(start_union, &walls, axis_index, snapped);
                mark_contact(&mut self.collision_contact, clamped, snapped);
                if let Some(node) = scene.get_mut(mover.id) {
                    set_axis_of(
                        &mut node.position,
                        axis_index,
                        axis_of(mover.start_pos, axis_index) + clamped,
                    );
                }
            }
            GizmoMode::Rotate => {
                let raw =
                    axis_of(node.rotation, axis_index) - axis_of(mover.start_rotation, axis_index);
                let snapped = building_mode::snap_scalar(raw, ROTATE_SNAP_STEP);
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
                        axis_of(mover.start_rotation, axis_index) + clamped,
                    );
                }
            }
            GizmoMode::Scale => {
                let raw = axis_of(node.scale, axis_index) - axis_of(mover.start_scale, axis_index);
                let snapped = building_mode::snap_scalar(raw, step);
                let sign = self.drag_face_sign();
                let (min_shift, max_shift) = if self.uniform_scale_by_default {
                    (-snapped * 0.5, snapped * 0.5)
                } else if sign >= 0.0 {
                    (0.0, snapped)
                } else {
                    (snapped, 0.0)
                };
                let (allowed_min, allowed_max) = building_mode::clamp_growth(
                    start_union,
                    &walls,
                    axis_index,
                    min_shift,
                    max_shift,
                );
                let allowed = if self.uniform_scale_by_default {
                    (allowed_max - allowed_min) * snapped.signum()
                } else if sign >= 0.0 {
                    allowed_max
                } else {
                    allowed_min
                };
                mark_contact(&mut self.collision_contact, allowed, snapped);
                if let Some(node) = scene.get_mut(mover.id) {
                    let start_axis = axis_of(mover.start_scale, axis_index).abs().max(0.01);
                    let new_axis = (start_axis + allowed).max(0.01);
                    let delta = new_axis - start_axis;
                    if self.uniform_scale_by_default {
                        let factor = (new_axis / start_axis).max(0.05);
                        node.scale = (mover.start_scale * factor).max(Vec3::splat(0.01));
                    } else {
                        let mut next = mover.start_scale;
                        set_axis_of(&mut next, axis_index, new_axis);
                        node.scale = next.max(Vec3::splat(0.01));
                        node.position =
                            mover.start_pos + axis_direction(axis_index) * (delta * 0.5 * sign);
                    }
                }
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
            mode: match self.render_style {
                NativeViewportRenderStyle::Solid => RenderMode::Solid,
                NativeViewportRenderStyle::Wireframe => RenderMode::Wireframe,
                NativeViewportRenderStyle::Preview => RenderMode::Preview,
            },
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
            triangle_budget: self.render_config.max_triangles,
            world_streaming_enabled: self.world_stream_config.enabled,
            world_stream_region_size: self.world_stream_config.region_size,
            world_stream_load_radius: self.world_stream_config.load_radius,
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
                    presentation_scale: self.gizmo_presentation_scale(),
                }
                .append_to(frame);
            }
            return;
        }
        let id = self.selected[0];
        let Some(node) = scene.get(id) else {
            return;
        };
        GizmoRenderSpec {
            mode: self.bridge.gizmo().mode,
            active_axis: self.bridge.highlighted_gizmo_axis(),
            active_scale_sign: self.bridge.highlighted_gizmo_scale_sign(),
            origin: scene.world_matrix(id).col(3).truncate(),
            entity_scale: node.scale,
            presentation_scale: self.gizmo_presentation_scale(),
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

    fn gizmo_presentation_scale(&self) -> f32 {
        if self.bridge.gizmo().mode == GizmoMode::Scale {
            1.0
        } else {
            (self.bridge.orbit_distance() / 5.0).max(1.0)
        }
    }
}

impl GameViewportPort for NativeGameViewportController {
    fn selected_ids(&self) -> Vec<SceneNodeId> {
        self.selected.clone()
    }

    fn set_selected_ids(&mut self, ids: Vec<SceneNodeId>) {
        self.selected = ids;
    }

    fn focus_entity(&mut self, scene: &SceneGraph, id: Option<SceneNodeId>) {
        if let Some(id) = id {
            self.selected = vec![id];
        }
        self.focus_selection(scene);
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
            node.rotation.y = 0.83;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Rotate, GizmoAxis::Y);
        let rotation = scene.get(mover).unwrap().rotation.y;
        assert!(
            (rotation - 0.785398).abs() < 1e-3,
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
            node.rotation.y = std::f32::consts::FRAC_PI_2;
        }
        controller.constrain_axis_drag(&mut scene, GizmoMode::Rotate, GizmoAxis::Y);
        let rotation = scene.get(mover).unwrap().rotation.y;
        assert!(
            (rotation - std::f32::consts::FRAC_PI_2).abs() < 1e-3,
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
}
