//! Input-side viewport helpers.
//!
//! This module owns the Tab Mode selection state, precise scene picking and
//! the projected vertex/edge/face overlay. The authoritative topology lives in
//! `SceneNode::editable_mesh`; this session only stores selection and drag
//! gesture state so undo and commands share one source of truth.

use std::collections::HashMap;
use std::sync::OnceLock;

use glam::{EulerRot, Mat4, Quat, Vec3};

use raf_core::scene::geometry::node_geometry_matrix;
use raf_core::scene::graph::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::scene::{EditableMeshData, WorldTransformCache};

use crate::bridge::picking_policy::{PickingLayer, PickingPolicy};
use crate::camera::Camera;
use crate::editable::EditableMesh;
use crate::geometry::mesh_data::MeshData;
use crate::geometry::primitives;
use crate::math::ray::{ray_sphere, ray_triangle, Ray};
use crate::math::transform;
use crate::projection;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MeshSelectMode {
    #[default]
    Vertex,
    Edge,
    Face,
}

impl MeshSelectMode {
    pub const fn slug(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Edge => "edge",
            Self::Face => "face",
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedEditEdge {
    pub start: [f32; 2],
    pub end: [f32; 2],
    pub selected: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedEditVertex {
    pub position: [f32; 2],
    pub selected: bool,
    pub hovered: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedEditFace {
    pub points: [[f32; 2]; 3],
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectedEditOverlay {
    pub edges: Vec<ProjectedEditEdge>,
    pub vertices: Vec<ProjectedEditVertex>,
    pub faces: Vec<ProjectedEditFace>,
}

#[derive(Debug, Default)]
pub struct ViewportEditSession {
    node: Option<SceneNodeId>,
    revision: u64,
    pub mode: MeshSelectMode,
    pub selected_vertices: Vec<u32>,
    pub selected_faces: Vec<u32>,
    hovered_vertex: Option<u32>,
    drag_active: bool,
    last_pointer: Option<[f32; 2]>,
    drag_vertices: Vec<u32>,
}

#[derive(Debug, Clone, Copy)]
struct RayPickCandidate {
    id: SceneNodeId,
    sphere_distance: f32,
    model: Mat4,
}

impl ViewportEditSession {
    /// Exact point/normal on the nearest supported primitive; ignores editor helpers.
    pub fn pick_surface(
        &self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        pointer: [f32; 2],
        size: [f32; 2],
    ) -> Option<(Vec3, Vec3)> {
        let id = self.pick_entity(scene, view_proj, pointer[0], pointer[1], size[0], size[1])?;
        let node = scene.get(id)?;
        let (origin, direction) = transform::screen_to_world_ray(
            pointer[0],
            pointer[1],
            size[0],
            size[1],
            &view_proj.inverse(),
        )?;
        let ray = Ray::new(origin, direction);
        let world = node_geometry_matrix(scene.world_matrix(id), node);
        self.with_pick_mesh(id, node, |mesh| {
            let mut nearest: Option<(f32, Vec3)> = None;
            for tri in mesh.indices.chunks_exact(3) {
                let a = world.transform_point3(mesh.positions[tri[0] as usize]);
                let b = world.transform_point3(mesh.positions[tri[1] as usize]);
                let c = world.transform_point3(mesh.positions[tri[2] as usize]);
                if let Some(t) = ray_triangle_two_sided(&ray, a, b, c) {
                    if nearest.is_none_or(|hit| t < hit.0) {
                        let mut normal = (b - a).cross(c - a).normalize_or_zero();
                        if normal.dot(direction) > 0.0 {
                            normal = -normal;
                        }
                        nearest = Some((t, normal));
                    }
                }
            }
            nearest.map(|(t, n)| (origin + direction * t, n))
        })
    }

    pub fn clear_drag_state(&mut self) {
        self.drag_active = false;
        self.last_pointer = None;
        self.drag_vertices.clear();
    }

    pub fn clear_selection(&mut self) {
        self.selected_vertices.clear();
        self.selected_faces.clear();
        self.hovered_vertex = None;
    }

    /// Leave Tab Mode without keeping the edited node pinned.
    pub fn release(&mut self) {
        self.clear_selection();
        self.clear_drag_state();
        self.node = None;
        self.revision = 0;
    }

    pub fn set_mode(&mut self, mode: MeshSelectMode) {
        if self.mode != mode {
            self.mode = mode;
            self.clear_selection();
        }
    }

    pub fn is_dragging(&self) -> bool {
        self.drag_active
    }

    pub fn node(&self) -> Option<SceneNodeId> {
        self.node
    }

    /// Keep the session anchored to the selected node and invalidate stale
    /// indices when an external command or script updates the topology.
    pub fn sync(&mut self, scene: &SceneGraph, selected: Option<SceneNodeId>) {
        if let Some(pinned) = self.node {
            // Tab Mode is focused on one piece: external selection changes must
            // not steal the active mesh. Only a removed/cleared mesh releases it.
            if !scene
                .get(pinned)
                .is_some_and(|node| node.editable_mesh.is_some())
            {
                self.release();
            }
        } else {
            self.node = selected.filter(|id| {
                scene
                    .get(*id)
                    .is_some_and(|node| node.editable_mesh.is_some())
            });
            self.revision = 0;
        }
        let Some(id) = self.node else {
            return;
        };
        let Some(mesh) = scene.get(id).and_then(|node| node.editable_mesh.as_ref()) else {
            self.node = None;
            self.clear_selection();
            return;
        };
        if mesh.revision != self.revision {
            self.revision = mesh.revision;
            let vertex_count = mesh.vertices.len() as u32;
            let face_count = mesh.face_count() as u32;
            self.selected_vertices.retain(|index| *index < vertex_count);
            self.selected_faces.retain(|index| *index < face_count);
            if self
                .hovered_vertex
                .is_some_and(|index| index >= vertex_count)
            {
                self.hovered_vertex = None;
            }
        }
    }

    pub fn prepare_selected_mesh(&mut self, scene: &SceneGraph, selected: Option<SceneNodeId>) {
        self.sync(scene, selected);
    }

    pub fn mesh<'a>(&self, scene: &'a SceneGraph) -> Option<&'a EditableMeshData> {
        let id = self.node?;
        scene.get(id)?.editable_mesh.as_ref()
    }

    pub fn select_all(&mut self, scene: &SceneGraph) {
        let Some(mesh) = self.mesh(scene) else {
            return;
        };
        match self.mode {
            MeshSelectMode::Vertex => {
                self.selected_vertices = (0..mesh.vertices.len() as u32).collect();
                self.selected_faces.clear();
            }
            MeshSelectMode::Edge => {
                self.selected_vertices = (0..mesh.vertices.len() as u32).collect();
                self.selected_faces.clear();
            }
            MeshSelectMode::Face => {
                self.selected_faces = (0..mesh.face_count() as u32).collect();
                self.selected_vertices = face_vertices(mesh, &self.selected_faces);
            }
        }
    }

    pub fn handle_selection_click(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        click_local: [f32; 2],
        shift: bool,
    ) -> bool {
        self.sync(scene, selected);
        let Some(mesh) = self.mesh(scene) else {
            return false;
        };
        let Some(id) = self.node else {
            return false;
        };
        let Some(node) = scene.get(id) else {
            return false;
        };
        let model = node_geometry_matrix(scene.world_matrix(id), node);
        let hit = match self.mode {
            MeshSelectMode::Vertex => pick_edit_vertex(
                mesh, &model, view_proj, vp_w, vp_h, click_local,
            )
            .map(EditHit::Vertex),
            MeshSelectMode::Edge => pick_edit_edge(mesh, &model, view_proj, vp_w, vp_h, click_local)
                .map(EditHit::Edge),
            MeshSelectMode::Face => pick_edit_face(
                mesh,
                &model,
                view_proj,
                vp_w,
                vp_h,
                click_local,
            )
            .map(EditHit::Face),
        };

        if !shift {
            self.selected_vertices.clear();
            self.selected_faces.clear();
        }
        match hit {
            Some(EditHit::Vertex(index)) => toggle_index(&mut self.selected_vertices, index),
            Some(EditHit::Edge([a, b])) => {
                toggle_index(&mut self.selected_vertices, a);
                toggle_index(&mut self.selected_vertices, b);
            }
            Some(EditHit::Face(face)) => {
                toggle_index(&mut self.selected_faces, face);
                if let Some(mesh) = self.mesh(scene) {
                    self.selected_vertices = face_vertices(mesh, &self.selected_faces);
                }
            }
            None => {}
        }
        self.hovered_vertex = None;
        true
    }

    /// Face and local-space impact point under the pointer. Tab Mode uses this
    /// to insert a movable point on the clicked triangle.
    pub fn pick_face_point(
        &self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        pointer: [f32; 2],
    ) -> Option<(u32, [f32; 3])> {
        let id = self.node?;
        let node = scene.get(id)?;
        let mesh = node.editable_mesh.as_ref()?;
        let model = node_geometry_matrix(scene.world_matrix(id), node);
        let (face, world_point) =
            pick_edit_face_hit(mesh, &model, view_proj, vp_w, vp_h, pointer)?;
        let local = model.inverse().transform_point3(world_point);
        local.is_finite().then_some((face, local.to_array()))
    }

    pub fn update_hover(
        &mut self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        pointer_local: [f32; 2],
    ) {
        let Some(mesh) = self.mesh(scene) else {
            self.hovered_vertex = None;
            return;
        };
        let Some(id) = self.node else {
            self.hovered_vertex = None;
            return;
        };
        let Some(node) = scene.get(id) else {
            self.hovered_vertex = None;
            return;
        };
        let model = node_geometry_matrix(scene.world_matrix(id), node);
        self.hovered_vertex = pick_edit_vertex(
            mesh,
            &model,
            view_proj,
            vp_w,
            vp_h,
            pointer_local,
        );
    }

    pub fn begin_drag(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        pointer_local: [f32; 2],
    ) {
        self.sync(scene, selected);
        let Some(mesh) = self.mesh(scene) else {
            return;
        };
        let Some(id) = self.node else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let model = node_geometry_matrix(scene.world_matrix(id), node);
        let over_handle = match self.mode {
            MeshSelectMode::Vertex => {
                pick_edit_vertex(mesh, &model, view_proj, vp_w, vp_h, pointer_local)
                    .is_some_and(|index| self.selected_vertices.contains(&index))
            }
            MeshSelectMode::Edge => pick_edit_edge(mesh, &model, view_proj, vp_w, vp_h, pointer_local)
                .is_some_and(|[a, b]| {
                    self.selected_vertices.contains(&a) && self.selected_vertices.contains(&b)
                }),
            MeshSelectMode::Face => pick_edit_face(mesh, &model, view_proj, vp_w, vp_h, pointer_local)
                .is_some_and(|face| self.selected_faces.contains(&face)),
        };
        if !over_handle {
            return;
        }
        self.drag_vertices = drag_targets(mesh, &self.selected_vertices, &self.selected_faces);
        if self.drag_vertices.is_empty() {
            return;
        }
        self.drag_active = true;
        self.last_pointer = Some(pointer_local);
    }

    /// Apply one drag step directly to the persisted topology.
    pub fn drag_selected_vertices(
        &mut self,
        scene: &mut SceneGraph,
        camera: &Camera,
        orbit_distance: f32,
        move_sensitivity: f32,
        current_pointer: [f32; 2],
        axis: Option<usize>,
    ) -> bool {
        if !self.drag_active {
            return false;
        }
        let Some(id) = self.node else {
            return false;
        };
        let Some(last_pointer) = self.last_pointer else {
            return false;
        };
        let delta = [
            current_pointer[0] - last_pointer[0],
            current_pointer[1] - last_pointer[1],
        ];
        self.last_pointer = Some(current_pointer);

        let Some(node) = scene.get(id) else {
            return false;
        };
        let forward = (camera.target - camera.position).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = Vec3::Y;
        // Pointer Y grows downward. Screen-right is +X and screen-up must be
        // +Y, so both axis signs matter and the old mapping was inverted.
        let mut world_delta = (right * delta[0] - up * delta[1])
            * (orbit_distance * 0.0015 * move_sensitivity.max(0.1));
        if let Some(axis) = axis {
            let rotation = Quat::from_euler(
                EulerRot::YXZ,
                node.rotation.y.to_radians(),
                node.rotation.x.to_radians(),
                node.rotation.z.to_radians(),
            );
            let direction = match axis {
                0 => rotation * Vec3::X,
                1 => rotation * Vec3::Y,
                _ => rotation * Vec3::Z,
            };
            world_delta = direction * world_delta.dot(direction);
        }
        let local_delta = world_delta_to_local(node, world_delta);
        if local_delta.length_squared() <= f32::EPSILON {
            return false;
        }
        let targets = std::mem::take(&mut self.drag_vertices);
        let moved = scene
            .get_mut(id)
            .and_then(|node| node.editable_mesh.as_mut())
            .is_some_and(|mesh| {
                let ok = mesh
                    .move_vertices(&targets, local_delta.to_array())
                    .is_ok();
                if ok {
                    mesh.touch();
                }
                ok
            });
        self.drag_vertices = targets;
        moved
    }

    pub fn project_overlay(
        &self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<ProjectedEditOverlay> {
        let id = self.node.or(selected)?;
        let node = scene.get(id)?;
        let mesh = node.editable_mesh.as_ref()?;
        let model = node_geometry_matrix(scene.world_matrix(id), node);

        let mut overlay = ProjectedEditOverlay::default();
        let mut seen_edges: Vec<(u32, u32)> = Vec::new();
        let selected_vertices = self
            .selected_vertices
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();
        let selected_faces = self
            .selected_faces
            .iter()
            .copied()
            .collect::<std::collections::HashSet<_>>();

        for (face_index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
            let face_selected = selected_faces.contains(&(face_index as u32));
            let mut projected = [[0.0f32; 2]; 3];
            let mut visible = true;
            for (slot, index) in triangle.iter().enumerate() {
                let world = model.transform_point3(Vec3::from_array(
                    mesh.vertices[*index as usize],
                ));
                match transform::project_point(world, view_proj, vp_w, vp_h) {
                    Some((screen, _)) => projected[slot] = screen,
                    None => visible = false,
                }
            }
            if visible && face_selected {
                overlay.faces.push(ProjectedEditFace {
                    points: projected,
                    selected: true,
                });
            }
            for edge in [
                (triangle[0], triangle[1]),
                (triangle[1], triangle[2]),
                (triangle[2], triangle[0]),
            ] {
                let key = if edge.0 < edge.1 {
                    (edge.0, edge.1)
                } else {
                    (edge.1, edge.0)
                };
                if seen_edges.contains(&key) {
                    continue;
                }
                seen_edges.push(key);
                let world_edge = [
                    model.transform_point3(Vec3::from_array(
                        mesh.vertices[key.0 as usize],
                    )),
                    model.transform_point3(Vec3::from_array(
                        mesh.vertices[key.1 as usize],
                    )),
                ];
                if let Some(projected) =
                    projection::project_edge(&world_edge, view_proj, vp_w, vp_h)
                {
                    overlay.edges.push(ProjectedEditEdge {
                        start: projected[0],
                        end: projected[1],
                        selected: face_selected
                            || (selected_vertices.contains(&key.0)
                                && selected_vertices.contains(&key.1)),
                    });
                }
            }
        }

        for (index, vertex) in mesh.vertices.iter().enumerate() {
            let world = model.transform_point3(Vec3::from_array(*vertex));
            if let Some((screen, _)) = transform::project_point(world, view_proj, vp_w, vp_h) {
                overlay.vertices.push(ProjectedEditVertex {
                    position: screen,
                    selected: selected_vertices.contains(&(index as u32)),
                    hovered: self.hovered_vertex == Some(index as u32),
                });
            }
        }

        Some(overlay)
    }

    /// Compatibility override for callers that still render a temporary mesh.
    /// Persisted editable topology is already handled by the scene renderer.
    pub fn mesh_override(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
    ) -> Option<(SceneNodeId, MeshData)> {
        self.sync(scene, selected);
        let id = self.node?;
        let mesh = scene.get(id)?.editable_mesh.as_ref()?;
        Some((
            id,
            EditableMesh::from_scene_data(mesh).to_mesh_data_uv(),
        ))
    }

    pub fn pick_entity(
        &self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        screen_x: f32,
        screen_y: f32,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<SceneNodeId> {
        self.pick_entity_with_policy(
            scene,
            view_proj,
            screen_x,
            screen_y,
            vp_w,
            vp_h,
            PickingPolicy::default(),
        )
    }

    /// Selects the nearest visible world object with a bounded broad phase and
    /// an optional precise triangle narrow phase.
    pub fn pick_entity_with_policy(
        &self,
        scene: &SceneGraph,
        view_proj: &Mat4,
        screen_x: f32,
        screen_y: f32,
        vp_w: f32,
        vp_h: f32,
        policy: PickingPolicy,
    ) -> Option<SceneNodeId> {
        if !policy.layer_mask.contains(PickingLayer::World) {
            return None;
        }

        let vp_inv = view_proj.inverse();
        let (ray_origin, ray_dir) =
            transform::screen_to_world_ray(screen_x, screen_y, vp_w, vp_h, &vp_inv)?;
        let ray = Ray::new(ray_origin, ray_dir);
        let transforms = WorldTransformCache::build(scene);
        let mut candidates = Vec::new();

        for (id, node) in scene.iter() {
            if !node.visible || node.name.is_empty() {
                continue;
            }
            if matches!(node.primitive, Primitive::Empty)
                && node.editable_mesh.is_none()
                && node.mesh_asset.is_none()
            {
                continue;
            }

            let world = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            let geometry = node_geometry_matrix(world, node);
            let center = geometry.col(3).truncate();
            let world_scale = Vec3::new(
                geometry.x_axis.truncate().length(),
                geometry.y_axis.truncate().length(),
                geometry.z_axis.truncate().length(),
            );
            let radius = self.pick_mesh_radius(id, node)
                * world_scale
                    .x
                    .max(world_scale.y)
                    .max(world_scale.z)
                    .max(0.01);

            if let Some(sphere_distance) = ray_sphere(&ray, center, radius.max(0.05)) {
                candidates.push(RayPickCandidate {
                    id,
                    sphere_distance,
                    model: geometry,
                });
            }
        }

        candidates.sort_by(|left, right| {
            left.sphere_distance
                .partial_cmp(&right.sphere_distance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        candidates.truncate(policy.max_cpu_candidates.max(1));
        let mut best: Option<(SceneNodeId, f32)> = None;

        for candidate in candidates {
            let Some(node) = scene.get(candidate.id) else {
                continue;
            };
            let hit_t = if policy.ray_triangle_narrow_phase {
                let triangle_hit = self.with_pick_mesh(candidate.id, node, |mesh| {
                    ray_hit_mesh(&ray, mesh, &candidate.model)
                });
                let Some(triangle_hit) = triangle_hit else {
                    continue;
                };
                triangle_hit
            } else {
                candidate.sphere_distance
            };

            if best.is_none() || hit_t < best.expect("best already checked").1 {
                best = Some((candidate.id, hit_t));
            }
        }

        best.map(|(id, _)| id)
    }

    fn with_pick_mesh<T>(
        &self,
        id: SceneNodeId,
        node: &SceneNode,
        callback: impl FnOnce(&MeshData) -> T,
    ) -> T {
        if let Some(mesh) = node.editable_mesh.as_ref() {
            let mesh_data = EditableMesh::from_scene_data(mesh).to_mesh_data_uv();
            return callback(&mesh_data);
        }

        callback(primitive_mesh_data(node.primitive))
    }

    fn pick_mesh_radius(&self, id: SceneNodeId, node: &SceneNode) -> f32 {
        self.with_pick_mesh(id, node, MeshData::bounding_radius)
    }
}

#[derive(Debug, Clone, Copy)]
enum EditHit {
    Vertex(u32),
    Edge([u32; 2]),
    Face(u32),
}

fn toggle_index(values: &mut Vec<u32>, index: u32) {
    if let Some(position) = values.iter().position(|value| *value == index) {
        values.remove(position);
    } else {
        values.push(index);
    }
}

fn face_vertices(mesh: &EditableMeshData, faces: &[u32]) -> Vec<u32> {
    let mut vertices = Vec::new();
    for &face in faces {
        let offset = face as usize * 3;
        if let Some(triangle) = mesh.indices.get(offset..offset + 3) {
            for &index in triangle {
                if !vertices.contains(&index) {
                    vertices.push(index);
                }
            }
        }
    }
    vertices
}

fn drag_targets(
    mesh: &EditableMeshData,
    vertices: &[u32],
    faces: &[u32],
) -> Vec<u32> {
    let mut targets = vertices.to_vec();
    for &index in &face_vertices(mesh, faces) {
        if !targets.contains(&index) {
            targets.push(index);
        }
    }
    targets
}

fn primitive_mesh_data(primitive: Primitive) -> &'static MeshData {
    static CUBE: OnceLock<MeshData> = OnceLock::new();
    static CAPSULE: OnceLock<MeshData> = OnceLock::new();
    static CYLINDER: OnceLock<MeshData> = OnceLock::new();
    static SPHERE: OnceLock<MeshData> = OnceLock::new();
    static PLANE: OnceLock<MeshData> = OnceLock::new();
    static WEDGE: OnceLock<MeshData> = OnceLock::new();
    static TORUS: OnceLock<MeshData> = OnceLock::new();
    static PRISM: OnceLock<MeshData> = OnceLock::new();
    static SPHERE2D: OnceLock<MeshData> = OnceLock::new();

    match primitive {
        Primitive::Cube | Primitive::Empty => CUBE.get_or_init(|| primitives::cube(1)),
        Primitive::Capsule => CAPSULE.get_or_init(|| primitives::capsule(16, 4)),
        Primitive::Cylinder => CYLINDER.get_or_init(|| primitives::cylinder(16)),
        Primitive::Sphere => SPHERE.get_or_init(|| primitives::sphere(8, 12)),
        Primitive::Plane => PLANE.get_or_init(|| primitives::plane(1)),
        Primitive::Wedge => WEDGE.get_or_init(|| primitives::wedge(1)),
        Primitive::Torus => TORUS.get_or_init(|| primitives::torus(24, 12)),
        Primitive::Prism => PRISM.get_or_init(|| primitives::prism(6)),
        Primitive::Sphere2D => SPHERE2D.get_or_init(|| primitives::sphere2d(32, 16)),
    }
}

fn pick_edit_vertex(
    mesh: &EditableMeshData,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<u32> {
    let mut best: Option<(u32, f32)> = None;

    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let world = model.transform_point3(Vec3::from_array(*vertex));
        let Some((screen, _)) = transform::project_point(world, view_proj, vp_w, vp_h) else {
            continue;
        };

        let dx = screen[0] - click_local[0];
        let dy = screen[1] - click_local[1];
        let distance = (dx * dx + dy * dy).sqrt();
        if distance > 10.0 {
            continue;
        }

        let better = match best {
            None => true,
            Some((_, best_distance)) => distance < best_distance,
        };
        if better {
            best = Some((index as u32, distance));
        }
    }

    best.map(|(index, _)| index)
}

fn pick_edit_edge(
    mesh: &EditableMeshData,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<[u32; 2]> {
    let mut best: Option<([u32; 2], f32)> = None;
    let mut seen: Vec<(u32, u32)> = Vec::new();
    for triangle in mesh.indices.chunks_exact(3) {
        for edge in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = if edge.0 < edge.1 {
                (edge.0, edge.1)
            } else {
                (edge.1, edge.0)
            };
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            let Some(a) = project_vertex(mesh, key.0, model, view_proj, vp_w, vp_h) else {
                continue;
            };
            let Some(b) = project_vertex(mesh, key.1, model, view_proj, vp_w, vp_h) else {
                continue;
            };
            let distance = point_segment_distance(click_local, a, b);
            if distance <= 8.0 && best.is_none_or(|(_, best)| distance < best) {
                best = Some(([edge.0, edge.1], distance));
            }
        }
    }
    best.map(|(edge, _)| edge)
}

fn pick_edit_face(
    mesh: &EditableMeshData,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<u32> {
    pick_edit_face_hit(mesh, model, view_proj, vp_w, vp_h, click_local).map(|(face, _)| face)
}

fn pick_edit_face_hit(
    mesh: &EditableMeshData,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<(u32, Vec3)> {
    let (origin, direction) = transform::screen_to_world_ray(
        click_local[0],
        click_local[1],
        vp_w,
        vp_h,
        &view_proj.inverse(),
    )?;
    let ray = Ray::new(origin, direction);
    let mut best: Option<(u32, f32)> = None;
    for (index, triangle) in mesh.indices.chunks_exact(3).enumerate() {
        let a = model.transform_point3(Vec3::from_array(mesh.vertices[triangle[0] as usize]));
        let b = model.transform_point3(Vec3::from_array(mesh.vertices[triangle[1] as usize]));
        let c = model.transform_point3(Vec3::from_array(mesh.vertices[triangle[2] as usize]));
        if let Some(t) = ray_triangle_two_sided(&ray, a, b, c) {
            if best.is_none_or(|(_, best)| t < best) {
                best = Some((index as u32, t));
            }
        }
    }
    best.map(|(index, t)| (index, origin + direction * t))
}

fn project_vertex(
    mesh: &EditableMeshData,
    index: u32,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
) -> Option<[f32; 2]> {
    let world = model.transform_point3(Vec3::from_array(
        *mesh.vertices.get(index as usize)?,
    ));
    transform::project_point(world, view_proj, vp_w, vp_h).map(|(screen, _)| screen)
}

fn point_segment_distance(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let ab = [b[0] - a[0], b[1] - a[1]];
    let ap = [point[0] - a[0], point[1] - a[1]];
    let length_squared = ab[0] * ab[0] + ab[1] * ab[1];
    if length_squared <= f32::EPSILON {
        return (ap[0] * ap[0] + ap[1] * ap[1]).sqrt();
    }
    let t = ((ap[0] * ab[0] + ap[1] * ab[1]) / length_squared).clamp(0.0, 1.0);
    let closest = [a[0] + ab[0] * t, a[1] + ab[1] * t];
    let dx = point[0] - closest[0];
    let dy = point[1] - closest[1];
    (dx * dx + dy * dy).sqrt()
}

fn world_delta_to_local(node: &SceneNode, world_delta: Vec3) -> Vec3 {
    let inverse_rotation = Quat::from_euler(
        EulerRot::YXZ,
        node.rotation.y.to_radians(),
        node.rotation.x.to_radians(),
        node.rotation.z.to_radians(),
    )
    .inverse();
    let local = inverse_rotation * world_delta;

    Vec3::new(
        local.x / node.scale.x.max(0.01),
        local.y / node.scale.y.max(0.01),
        local.z / node.scale.z.max(0.01),
    )
}

fn ray_hit_mesh(ray: &Ray, mesh: &MeshData, model: &Mat4) -> Option<f32> {
    let mut best: Option<f32> = None;

    for tri in mesh.indices.chunks_exact(3) {
        let a = (*model * mesh.positions[tri[0] as usize].extend(1.0)).truncate();
        let b = (*model * mesh.positions[tri[1] as usize].extend(1.0)).truncate();
        let c = (*model * mesh.positions[tri[2] as usize].extend(1.0)).truncate();

        if let Some(t) = ray_triangle_two_sided(ray, a, b, c) {
            if best.is_none() || t < best.expect("best already checked") {
                best = Some(t);
            }
        }
    }

    best
}

fn ray_triangle_two_sided(ray: &Ray, a: Vec3, b: Vec3, c: Vec3) -> Option<f32> {
    ray_triangle(ray, a, b, c).or_else(|| ray_triangle(ray, a, c, b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::picking_policy::PickingLayerMask;

    #[test]
    fn editor_mesh_pick_accepts_visible_surface_from_either_winding() {
        let plane = primitives::plane(1);
        let above = Ray::new(Vec3::new(0.0, 1.0, 0.0), Vec3::NEG_Y);
        let below = Ray::new(Vec3::new(0.0, -1.0, 0.0), Vec3::Y);

        assert!(ray_hit_mesh(&above, &plane, &Mat4::IDENTITY).is_some());
        assert!(ray_hit_mesh(&below, &plane, &Mat4::IDENTITY).is_some());
    }

    #[test]
    fn primitive_bounds_use_the_actual_mesh_radius() {
        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let session = ViewportEditSession::default();
        let node = scene.get(cube).expect("cube node");

        assert!(session.pick_mesh_radius(cube, node) > 0.8);
    }

    #[test]
    fn world_picking_respects_the_layer_mask() {
        let session = ViewportEditSession::default();
        let policy = PickingPolicy {
            layer_mask: PickingLayerMask::empty(),
            ..PickingPolicy::default()
        };

        assert_eq!(
            session.pick_entity_with_policy(
                &SceneGraph::new(),
                &Mat4::IDENTITY,
                10.0,
                10.0,
                100.0,
                100.0,
                policy,
            ),
            None
        );
    }

    #[test]
    fn mesh_drag_follows_screen_right_and_screen_up() {
        let mut scene = SceneGraph::new();
        let id = scene.add_root_with_primitive("Cube", Primitive::Cube);
        scene.get_mut(id).unwrap().editable_mesh = Some(
            EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5))
                .expect("box mesh"),
        );
        let camera = Camera::default();
        let view_proj = camera.view_projection(800.0, 600.0);
        let mut session = ViewportEditSession::default();
        session.prepare_selected_mesh(&scene, Some(id));
        session.set_mode(MeshSelectMode::Vertex);
        session.selected_vertices = vec![0];
        let model = node_geometry_matrix(scene.world_matrix(id), scene.get(id).unwrap());
        let world = model.transform_point3(Vec3::new(-0.5, -0.5, -0.5));
        let (screen, _) = transform::project_point(world, &view_proj, 800.0, 600.0)
            .expect("projected vertex");
        session.begin_drag(&scene, Some(id), &view_proj, 800.0, 600.0, screen);
        assert!(session.is_dragging(), "drag did not start on the projected vertex");

        let before = scene
            .get(id)
            .unwrap()
            .editable_mesh
            .as_ref()
            .unwrap()
            .vertex_position(0)
            .unwrap();
        assert!(session.drag_selected_vertices(
            &mut scene,
            &camera,
            5.0,
            1.0,
            [screen[0] + 24.0, screen[1] - 24.0],
            None,
        ));
        let after = scene
            .get(id)
            .unwrap()
            .editable_mesh
            .as_ref()
            .unwrap()
            .vertex_position(0)
            .unwrap();
        assert!(after.x > before.x, "screen-right must move +X");
        assert!(after.y > before.y, "screen-up must move +Y");
    }

    #[test]
    fn face_selection_collects_unique_vertices() {
        let mesh = EditableMeshData::box_mesh(Vec3::splat(-0.5), Vec3::splat(0.5))
            .expect("box mesh");
        let vertices = face_vertices(&mesh, &[0, 1]);
        assert_eq!(vertices.len(), 4);
        assert!(vertices.iter().all(|index| *index < mesh.vertices.len() as u32));
    }
}
