//! Input-side viewport helpers.
//!
//! This module owns temporary editable mesh state, precise scene picking,
//! and projected overlay data for edit mode. It is renderer-side so the
//! retained surface can stay thin and mostly paint precomputed data.

use std::collections::HashMap;
use std::sync::OnceLock;

use glam::{EulerRot, Mat4, Quat, Vec3};

use raf_core::scene::graph::{Primitive, SceneGraph, SceneNode, SceneNodeId};
use raf_core::scene::WorldTransformCache;

use crate::bridge::picking_policy::{PickingLayer, PickingPolicy};
use crate::camera::Camera;
use crate::editable::EditableMesh;
use crate::geometry::mesh_data::MeshData;
use crate::geometry::primitives;
use crate::math::ray::{ray_sphere, ray_triangle, Ray};
use crate::math::transform;
use crate::projection;

#[derive(Debug, Clone, Copy)]
pub struct ProjectedEditEdge {
    pub start: [f32; 2],
    pub end: [f32; 2],
}

#[derive(Debug, Clone, Copy)]
pub struct ProjectedEditVertex {
    pub position: [f32; 2],
    pub selected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProjectedEditOverlay {
    pub edges: Vec<ProjectedEditEdge>,
    pub vertices: Vec<ProjectedEditVertex>,
}

#[derive(Debug, Default)]
pub struct ViewportEditSession {
    editable_meshes: HashMap<SceneNodeId, EditableMesh>,
    drag_active: bool,
    last_pointer: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Copy)]
struct RayPickCandidate {
    id: SceneNodeId,
    sphere_distance: f32,
    model: Mat4,
}

impl ViewportEditSession {
    pub fn clear_drag_state(&mut self) {
        self.drag_active = false;
        self.last_pointer = None;
    }

    pub fn prepare_selected_mesh(&mut self, scene: &SceneGraph, selected: Option<SceneNodeId>) {
        let Some(id) = selected else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let _ = self.ensure_edit_mesh(id, node);
    }

    pub fn mesh_override(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
    ) -> Option<(SceneNodeId, MeshData)> {
        let id = selected?;
        let node = scene.get(id)?;
        let mesh = self.ensure_edit_mesh(id, node);
        Some((id, mesh.to_mesh_data()))
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
        let Some(id) = selected else {
            return false;
        };
        let Some(node) = scene.get(id) else {
            return false;
        };
        let model = scene.world_matrix(id);
        let mesh = self.ensure_edit_mesh(id, node);

        if let Some(vertex_idx) = pick_edit_vertex(mesh, &model, view_proj, vp_w, vp_h, click_local)
        {
            if !shift {
                mesh.selection.clear();
            }
            mesh.selection.toggle_vertex(vertex_idx);
            true
        } else if !shift {
            mesh.selection.clear();
            true
        } else {
            false
        }
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
        let Some(id) = selected else {
            return;
        };
        let Some(node) = scene.get(id) else {
            return;
        };
        let model = scene.world_matrix(id);

        let can_start_drag = {
            let mesh = self.ensure_edit_mesh(id, node);
            pick_edit_vertex(mesh, &model, view_proj, vp_w, vp_h, pointer_local).is_some()
                && !mesh.selection.vertices.is_empty()
        };

        if can_start_drag {
            self.drag_active = true;
            self.last_pointer = Some(pointer_local);
        }
    }

    pub fn drag_selected_vertices(
        &mut self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        camera: &Camera,
        orbit_distance: f32,
        move_sensitivity: f32,
        current_pointer: [f32; 2],
    ) -> bool {
        if !self.drag_active {
            return false;
        }

        let Some(id) = selected else {
            return false;
        };
        let Some(node) = scene.get(id) else {
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

        let forward = (camera.target - camera.position).normalize_or_zero();
        let right = forward.cross(Vec3::Y).normalize_or_zero();
        let up = Vec3::Y;
        let world_delta = (-right * delta[0] + up * delta[1])
            * (orbit_distance * 0.0015 * move_sensitivity.max(0.1));
        let local_delta = world_delta_to_local(node, world_delta);

        if let Some(mesh) = self.editable_meshes.get_mut(&id) {
            mesh.move_selected(local_delta);
            return true;
        }

        false
    }

    pub fn project_overlay(
        &self,
        scene: &SceneGraph,
        selected: Option<SceneNodeId>,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
    ) -> Option<ProjectedEditOverlay> {
        let id = selected?;
        let mesh = self.editable_meshes.get(&id)?;
        let model = scene.world_matrix(id);

        let mut overlay = ProjectedEditOverlay::default();

        for edge in mesh.wireframe_edges() {
            let world_edge = [
                (model * edge[0].extend(1.0)).truncate(),
                (model * edge[1].extend(1.0)).truncate(),
            ];
            if let Some(projected) = projection::project_edge(&world_edge, view_proj, vp_w, vp_h) {
                overlay.edges.push(ProjectedEditEdge {
                    start: projected[0],
                    end: projected[1],
                });
            }
        }

        for (index, vertex) in mesh.vertices.iter().enumerate() {
            let world = (model * vertex.position.extend(1.0)).truncate();
            if let Some((screen, _)) = transform::project_point(world, view_proj, vp_w, vp_h) {
                overlay.vertices.push(ProjectedEditVertex {
                    position: screen,
                    selected: mesh.selection.vertices.contains(&index),
                });
            }
        }

        Some(overlay)
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
            if matches!(node.primitive, Primitive::Empty) {
                continue;
            }

            let world = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            let center = world.col(3).truncate();
            let world_scale = Vec3::new(
                world.x_axis.truncate().length(),
                world.y_axis.truncate().length(),
                world.z_axis.truncate().length(),
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
                    model: world,
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

    fn ensure_edit_mesh(&mut self, id: SceneNodeId, node: &SceneNode) -> &mut EditableMesh {
        self.editable_meshes
            .entry(id)
            .or_insert_with(|| match node.primitive {
                Primitive::Cube => EditableMesh::cube(),
                Primitive::Plane => EditableMesh::plane(),
                Primitive::Cylinder => EditableMesh::cylinder(16),
                Primitive::Sphere => EditableMesh::sphere(8, 12),
                Primitive::Empty => EditableMesh::cube(),
            })
    }

    fn with_pick_mesh<T>(
        &self,
        id: SceneNodeId,
        node: &SceneNode,
        callback: impl FnOnce(&MeshData) -> T,
    ) -> T {
        if let Some(mesh) = self.editable_meshes.get(&id) {
            let mesh_data = mesh.to_mesh_data();
            return callback(&mesh_data);
        }

        callback(primitive_mesh_data(node.primitive))
    }

    fn pick_mesh_radius(&self, id: SceneNodeId, node: &SceneNode) -> f32 {
        self.with_pick_mesh(id, node, MeshData::bounding_radius)
    }
}

fn primitive_mesh_data(primitive: Primitive) -> &'static MeshData {
    static CUBE: OnceLock<MeshData> = OnceLock::new();
    static CYLINDER: OnceLock<MeshData> = OnceLock::new();
    static SPHERE: OnceLock<MeshData> = OnceLock::new();
    static PLANE: OnceLock<MeshData> = OnceLock::new();

    match primitive {
        Primitive::Cube | Primitive::Empty => CUBE.get_or_init(|| primitives::cube(1)),
        Primitive::Cylinder => CYLINDER.get_or_init(|| primitives::cylinder(16)),
        Primitive::Sphere => SPHERE.get_or_init(|| primitives::sphere(8, 12)),
        Primitive::Plane => PLANE.get_or_init(|| primitives::plane(1)),
    }
}

fn pick_edit_vertex(
    mesh: &EditableMesh,
    model: &Mat4,
    view_proj: &Mat4,
    vp_w: f32,
    vp_h: f32,
    click_local: [f32; 2],
) -> Option<usize> {
    let mut best: Option<(usize, f32)> = None;

    for (index, vertex) in mesh.vertices.iter().enumerate() {
        let world = (*model * vertex.position.extend(1.0)).truncate();
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
            best = Some((index, distance));
        }
    }

    best.map(|(index, _)| index)
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
}
