//! Scene renderer: orchestrates the full render pipeline.
//!
//! Takes a scene graph, camera parameters, and viewport dimensions,
//! and produces an RGBA pixel buffer ready for display.
//!
//! Pipeline stages:
//! 1. Frustum cull (skip objects outside view)
//! 2. Generate/cache mesh data per primitive type
//! 3. Transform vertices: Object -> World -> Clip -> Screen
//! 4. Backface cull + clip against near plane
//! 5. Shade each triangle (flat shading)
//! 6. Rasterize with scanline + Z-buffer
//! 7. Output pixel buffer
//!
//! This module lives in raf_render and has no egui dependency.
//! The editor's viewport bridge uploads the pixel buffer to egui.

use glam::{Mat4, Vec3, Vec4};
use std::collections::HashSet;
use std::sync::Arc;

use crate::api_graphic_basic::command_list::{BasicCommandList, GraphicCommand};
use crate::api_graphic_basic::grid::{build_3d_grid, GridLineKind};
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::camera::{Camera, CameraMode};
use crate::geometry::mesh_data::MeshData;
use crate::geometry::primitives;
use crate::math::frustum::Frustum;
use crate::math::transform;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::render_pipeline::rasterizer::{self, ScreenVertex};

use raf_core::scene::graph::{Primitive, SceneGraph, SceneNodeId};

/// Render statistics for the current frame.
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    /// Total entities in the scene.
    pub total_entities: u32,
    /// Entities visible after frustum cull.
    pub visible_entities: u32,
    /// Total triangles submitted to the rasterizer.
    pub triangles_rendered: u32,
    /// Triangles culled by backface test.
    pub triangles_culled: u32,
}

/// Render mode for the viewport.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenderMode {
    /// Fill triangles with flat shading.
    Solid,
    /// Draw only triangle edges.
    Wireframe,
    /// Solid + wireframe edges on all objects.
    Preview,
}

/// Per-frame renderer options supplied by the editor viewport.
///
/// These values keep the scene renderer independent from egui while still
/// allowing the editor to control presentation details such as solid edges,
/// xray opacity, and selection highlighting.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
    pub mode: RenderMode,
    pub show_grid_3d: bool,
    pub grid_spacing: f32,
    pub grid_load_distance: f32,
    pub solid_show_surface_edges: bool,
    pub solid_xray_mode: bool,
    pub solid_face_tonality: bool,
    pub selection_outline: bool,
    pub selection_outline_color: [u8; 4],
    /// Outline color for secondary (non-primary) selected entities.
    /// Used when multiple entities are selected to distinguish the primary
    /// from the rest, like Unity/Blender do.
    pub secondary_selection_outline_color: [u8; 4],
    /// Entity ID of the primary selection (first in the multi-select list).
    /// When None or not found, all selected entities use the primary color.
    pub primary_selected: Option<u64>,
    pub grid_y: f32,
    pub grid_no_depth_test: bool,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            mode: RenderMode::Solid,
            show_grid_3d: true,
            grid_spacing: 1.0,
            grid_load_distance: 15.0,
            solid_show_surface_edges: false,
            solid_xray_mode: false,
            solid_face_tonality: true,
            selection_outline: true,
            selection_outline_color: [255, 160, 40, 255],
            secondary_selection_outline_color: [255, 120, 20, 180],
            primary_selected: None,
            grid_y: -0.02,
            grid_no_depth_test: false,
        }
    }
}

/// The CPU scene renderer.
///
/// Owns the framebuffer and mesh cache. Stateless between frames
/// except for the framebuffer allocation (reused across frames).
pub struct SceneRenderer {
    /// The render target.
    framebuffer: Framebuffer,
    /// Cached mesh data per primitive type.
    cube_mesh: MeshData,
    cube_basic_mesh: Arc<BasicMesh>,
    cube_edges: Vec<[Vec3; 2]>,
    cube_radius: f32,
    cylinder_mesh: MeshData,
    cylinder_basic_mesh: Arc<BasicMesh>,
    cylinder_edges: Vec<[Vec3; 2]>,
    cylinder_radius: f32,
    sphere_mesh: MeshData,
    sphere_basic_mesh: Arc<BasicMesh>,
    sphere_edges: Vec<[Vec3; 2]>,
    sphere_radius: f32,
    plane_mesh: MeshData,
    plane_basic_mesh: Arc<BasicMesh>,
    plane_edges: Vec<[Vec3; 2]>,
    plane_radius: f32,
    /// Stats from the last frame.
    pub stats: FrameStats,
}

#[derive(Debug, Clone)]
pub struct SceneRenderFrame {
    pub commands: BasicCommandList,
    pub view_proj: Mat4,
    pub light_dir: Vec3,
    pub width: u32,
    pub height: u32,
    pub stats: FrameStats,
}

#[derive(Debug, Clone, Copy)]
struct GridBounds {
    min_x: f32,
    max_x: f32,
    min_z: f32,
    max_z: f32,
}

impl GridBounds {
    fn from_center_radius(center: Vec3, radius: f32) -> Self {
        Self {
            min_x: center.x - radius,
            max_x: center.x + radius,
            min_z: center.z - radius,
            max_z: center.z + radius,
        }
    }

    fn expand_with(&mut self, center: Vec3, radius: f32) {
        self.min_x = self.min_x.min(center.x - radius);
        self.max_x = self.max_x.max(center.x + radius);
        self.min_z = self.min_z.min(center.z - radius);
        self.max_z = self.max_z.max(center.z + radius);
    }
}

impl SceneRenderer {
    /// Create a new renderer with initial viewport dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let cube_mesh = primitives::cube(1);
        let cylinder_mesh = primitives::cylinder(32);
        let sphere_mesh = primitives::sphere(16, 24);
        let plane_mesh = primitives::plane(1);

        Self {
            framebuffer: Framebuffer::new(width.max(1), height.max(1)),
            cube_basic_mesh: Arc::new(mesh_to_basic(&cube_mesh)),
            cube_edges: primitives::extract_edges(&cube_mesh),
            cube_radius: cube_mesh.bounding_radius(),
            cube_mesh,
            cylinder_basic_mesh: Arc::new(mesh_to_basic(&cylinder_mesh)),
            cylinder_edges: primitives::extract_edges(&cylinder_mesh),
            cylinder_radius: cylinder_mesh.bounding_radius(),
            cylinder_mesh,
            sphere_basic_mesh: Arc::new(mesh_to_basic(&sphere_mesh)),
            sphere_edges: primitives::extract_edges(&sphere_mesh),
            sphere_radius: sphere_mesh.bounding_radius(),
            sphere_mesh,
            plane_basic_mesh: Arc::new(mesh_to_basic(&plane_mesh)),
            plane_edges: primitives::extract_edges(&plane_mesh),
            plane_radius: plane_mesh.bounding_radius(),
            plane_mesh,
            stats: FrameStats::default(),
        }
    }

    /// Render the scene and return the pixel buffer.
    ///
    /// This is the main entry point. Call once per frame.
    pub fn render(
        &mut self,
        scene: &SceneGraph,
        camera: &Camera,
        vp_w: f32,
        vp_h: f32,
        selected: &[SceneNodeId],
        bg_color: [u8; 4],
        light_dir: Vec3,
        options: RenderOptions,
        mesh_override: Option<(SceneNodeId, &MeshData)>,
    ) -> &[u8] {
        let w = (vp_w as u32).max(1);
        let h = (vp_h as u32).max(1);

        self.framebuffer.resize(w, h);
        self.framebuffer
            .clear(bg_color[0], bg_color[1], bg_color[2], bg_color[3]);

        let view = camera.view_matrix();
        let proj = camera.projection_matrix(vp_w, vp_h);
        let vp = proj * view;
        let frustum = Frustum::from_matrix(&vp);
        let light_dir = light_dir.normalize();
        let cam_eye = camera.eye();
        let selected_ids: HashSet<_> = selected.iter().copied().collect();

        let cube_mesh = &self.cube_mesh;
        let cube_edges = self.cube_edges.as_slice();
        let cube_radius = self.cube_radius;
        let cylinder_mesh = &self.cylinder_mesh;
        let cylinder_edges = self.cylinder_edges.as_slice();
        let cylinder_radius = self.cylinder_radius;
        let sphere_mesh = &self.sphere_mesh;
        let sphere_edges = self.sphere_edges.as_slice();
        let sphere_radius = self.sphere_radius;
        let plane_mesh = &self.plane_mesh;
        let plane_edges = self.plane_edges.as_slice();
        let plane_radius = self.plane_radius;

        let mut stats = FrameStats::default();
        let mut grid_bounds: Option<GridBounds> = None;

        // Collect render jobs first (avoids borrow conflict on self)
        let mut jobs: Vec<RenderJob> = Vec::new();

        for (id, node) in scene.iter() {
            if !node.visible || node.name.is_empty() {
                continue;
            }
            if matches!(node.primitive, Primitive::Empty | Primitive::Sprite2D) {
                continue;
            }

            stats.total_entities += 1;

            // World matrix (includes parent chain)
            let model = scene.world_matrix(id);
            let world_pos = model.col(3).truncate();

            let mesh_radius = match node.primitive {
                Primitive::Cube => cube_radius,
                Primitive::Cylinder => cylinder_radius,
                Primitive::Sphere => sphere_radius,
                Primitive::Plane => plane_radius,
                _ => cube_radius,
            };

            let bounding_r = mesh_radius
                * node
                    .scale
                    .x
                    .abs()
                    .max(node.scale.y.abs())
                    .max(node.scale.z.abs());

            if let Some(bounds) = &mut grid_bounds {
                bounds.expand_with(world_pos, bounding_r);
            } else {
                grid_bounds = Some(GridBounds::from_center_radius(world_pos, bounding_r));
            }

            if !frustum.intersects_sphere(world_pos, bounding_r) {
                continue;
            }

            stats.visible_entities += 1;

            let mut base_color = [node.color.r, node.color.g, node.color.b, node.color.a];
            if matches!(options.mode, RenderMode::Solid) && options.solid_xray_mode {
                // Clamp opaque objects so the depth-tested scene becomes easier to inspect.
                base_color[3] = base_color[3].min(120);
            }
            let is_selected = selected_ids.contains(&id);

            let dist = (world_pos - cam_eye).length();
            let is_transparent = node.color.a < 255;

            jobs.push(RenderJob {
                id,
                primitive: node.primitive,
                model,
                base_color,
                is_selected,
                dist_to_camera: dist,
                is_transparent,
            });
        }

        if options.show_grid_3d && matches!(camera.mode, CameraMode::Perspective) {
            draw_world_grid(
                &mut self.framebuffer,
                camera,
                vp_w,
                vp_h,
                options.grid_spacing,
                options.grid_load_distance,
                grid_bounds,
            );
        }

        // Sort: opaque first (front-to-back for early Z rejection),
        // then transparent (back-to-front for correct blending).
        jobs.sort_by(|a, b| match (a.is_transparent, b.is_transparent) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (false, false) => a
                .dist_to_camera
                .partial_cmp(&b.dist_to_camera)
                .unwrap_or(std::cmp::Ordering::Equal),
            (true, true) => b
                .dist_to_camera
                .partial_cmp(&a.dist_to_camera)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        // Execute render jobs (now we can borrow framebuffer mutably)
        let use_tonality =
            !(matches!(options.mode, RenderMode::Solid) && !options.solid_face_tonality);
        for job in &jobs {
            let override_mesh = mesh_override.and_then(|(override_id, override_mesh)| {
                (override_id == job.id).then_some(override_mesh)
            });
            let override_edges = override_mesh.map(primitives::extract_edges);

            let (mesh, edges): (&MeshData, &[[Vec3; 2]]) =
                if let Some(override_mesh) = override_mesh {
                    (override_mesh, override_edges.as_deref().unwrap_or(&[]))
                } else {
                    match job.primitive {
                        Primitive::Cube => (cube_mesh, cube_edges),
                        Primitive::Cylinder => (cylinder_mesh, cylinder_edges),
                        Primitive::Sphere => (sphere_mesh, sphere_edges),
                        Primitive::Plane => (plane_mesh, plane_edges),
                        _ => (cube_mesh, cube_edges),
                    }
                };

            let mvp = vp * job.model;
            let normal_mat = transform::normal_matrix(&job.model);

            let color = if job.is_selected {
                [
                    job.base_color[0].saturating_add(30),
                    job.base_color[1].saturating_add(30),
                    job.base_color[2].saturating_add(30),
                    job.base_color[3],
                ]
            } else {
                job.base_color
            };

            // Process each triangle
            for tri_idx in (0..mesh.indices.len()).step_by(3) {
                let i0 = mesh.indices[tri_idx] as usize;
                let i1 = mesh.indices[tri_idx + 1] as usize;
                let i2 = mesh.indices[tri_idx + 2] as usize;

                let p0 = mesh.positions[i0];
                let p1 = mesh.positions[i1];
                let p2 = mesh.positions[i2];

                // Transform to clip space
                let c0 = mvp * Vec4::new(p0.x, p0.y, p0.z, 1.0);
                let c1 = mvp * Vec4::new(p1.x, p1.y, p1.z, 1.0);
                let c2 = mvp * Vec4::new(p2.x, p2.y, p2.z, 1.0);

                // Near plane clip: skip if any vertex behind camera
                if c0.w <= 0.001 || c1.w <= 0.001 || c2.w <= 0.001 {
                    stats.triangles_culled += 1;
                    continue;
                }

                // Perspective divide -> NDC -> screen
                let shade0 = if use_tonality {
                    0.3 + 0.7
                        * transform::transform_normal(mesh.normals[i0], &normal_mat)
                            .dot(light_dir)
                            .max(0.0)
                } else {
                    1.0
                };
                let shade1 = if use_tonality {
                    0.3 + 0.7
                        * transform::transform_normal(mesh.normals[i1], &normal_mat)
                            .dot(light_dir)
                            .max(0.0)
                } else {
                    1.0
                };
                let shade2 = if use_tonality {
                    0.3 + 0.7
                        * transform::transform_normal(mesh.normals[i2], &normal_mat)
                            .dot(light_dir)
                            .max(0.0)
                } else {
                    1.0
                };

                let to_screen = |c: Vec4, shade: f32| -> ScreenVertex {
                    let inv_w = 1.0 / c.w;
                    let ndc_x = c.x * inv_w;
                    let ndc_y = c.y * inv_w;
                    let ndc_z = c.z * inv_w;
                    ScreenVertex {
                        x: (ndc_x + 1.0) * 0.5 * vp_w,
                        y: (1.0 - ndc_y) * 0.5 * vp_h,
                        z: (ndc_z + 1.0) * 0.5,
                        shade,
                    }
                };

                let sv0 = to_screen(c0, shade0);
                let sv1 = to_screen(c1, shade1);
                let sv2 = to_screen(c2, shade2);

                // Rasterize based on render mode.
                match options.mode {
                    RenderMode::Wireframe => {
                        // Skip filled triangles in wireframe mode.
                    }
                    RenderMode::Solid | RenderMode::Preview => {
                        if job.is_transparent {
                            rasterizer::rasterize_triangle_blended(
                                &mut self.framebuffer,
                                sv0,
                                sv1,
                                sv2,
                                color[0],
                                color[1],
                                color[2],
                                color[3],
                            );
                        } else {
                            rasterizer::rasterize_triangle(
                                &mut self.framebuffer,
                                sv0,
                                sv1,
                                sv2,
                                color[0],
                                color[1],
                                color[2],
                                color[3],
                            );
                        }
                    }
                }

                stats.triangles_rendered += 1;
            }

            let draw_surface_edges = match options.mode {
                RenderMode::Wireframe => true,
                RenderMode::Preview => true,
                RenderMode::Solid => options.solid_show_surface_edges,
            };

            if draw_surface_edges || (options.selection_outline && job.is_selected) {
                let edge_color = if job.is_selected && options.selection_outline {
                    // Distinguish primary from secondary selection when
                    // multiple entities are selected (Unity/Blender style).
                    let is_primary = options.primary_selected == Some(job.id.0 as u64);
                    if is_primary {
                        options.selection_outline_color
                    } else {
                        options.secondary_selection_outline_color
                    }
                } else {
                    surface_edge_color(job.base_color)
                };
                draw_wireframe_overlay(&mut self.framebuffer, edges, &mvp, vp_w, vp_h, edge_color);
            }
        }

        self.stats = stats;
        self.framebuffer.pixels()
    }

    pub fn build_frame(
        &mut self,
        scene: &SceneGraph,
        camera: &Camera,
        vp_w: f32,
        vp_h: f32,
        selected: &[SceneNodeId],
        bg_color: [u8; 4],
        light_dir: Vec3,
        options: RenderOptions,
        mesh_override: Option<(SceneNodeId, &MeshData)>,
    ) -> SceneRenderFrame {
        let w = (vp_w as u32).max(1);
        let h = (vp_h as u32).max(1);

        let view = camera.view_matrix();
        let proj = camera.projection_matrix(vp_w, vp_h);
        let vp = proj * view;
        let frustum = Frustum::from_matrix(&vp);
        let light_dir = light_dir.normalize();
        let cam_eye = camera.eye();
        let selected_ids: HashSet<_> = selected.iter().copied().collect();

        let cube_mesh = &self.cube_mesh;
        let cube_basic_mesh = Arc::clone(&self.cube_basic_mesh);
        let cube_edges = self.cube_edges.as_slice();
        let cube_radius = self.cube_radius;
        let cylinder_mesh = &self.cylinder_mesh;
        let cylinder_basic_mesh = Arc::clone(&self.cylinder_basic_mesh);
        let cylinder_edges = self.cylinder_edges.as_slice();
        let cylinder_radius = self.cylinder_radius;
        let sphere_mesh = &self.sphere_mesh;
        let sphere_basic_mesh = Arc::clone(&self.sphere_basic_mesh);
        let sphere_edges = self.sphere_edges.as_slice();
        let sphere_radius = self.sphere_radius;
        let plane_mesh = &self.plane_mesh;
        let plane_basic_mesh = Arc::clone(&self.plane_basic_mesh);
        let plane_edges = self.plane_edges.as_slice();
        let plane_radius = self.plane_radius;

        let mut stats = FrameStats::default();
        let mut grid_bounds: Option<GridBounds> = None;
        let mut commands = BasicCommandList::new();
        commands.clear(bg_color);

        let mut jobs: Vec<RenderJob> = Vec::new();

        for (id, node) in scene.iter() {
            if !node.visible || node.name.is_empty() {
                continue;
            }
            if matches!(node.primitive, Primitive::Empty | Primitive::Sprite2D) {
                continue;
            }

            stats.total_entities += 1;

            let model = scene.world_matrix(id);
            let world_pos = model.col(3).truncate();

            let mesh_radius = match node.primitive {
                Primitive::Cube => cube_radius,
                Primitive::Cylinder => cylinder_radius,
                Primitive::Sphere => sphere_radius,
                Primitive::Plane => plane_radius,
                _ => cube_radius,
            };

            let bounding_r = mesh_radius
                * node
                    .scale
                    .x
                    .abs()
                    .max(node.scale.y.abs())
                    .max(node.scale.z.abs());

            if let Some(bounds) = &mut grid_bounds {
                bounds.expand_with(world_pos, bounding_r);
            } else {
                grid_bounds = Some(GridBounds::from_center_radius(world_pos, bounding_r));
            }

            if !frustum.intersects_sphere(world_pos, bounding_r) {
                continue;
            }

            stats.visible_entities += 1;

            let mut base_color = [node.color.r, node.color.g, node.color.b, node.color.a];
            if matches!(options.mode, RenderMode::Solid) && options.solid_xray_mode {
                base_color[3] = base_color[3].min(120);
            }
            let is_selected = selected_ids.contains(&id);

            let dist = (world_pos - cam_eye).length();
            let is_transparent = node.color.a < 255;

            jobs.push(RenderJob {
                id,
                primitive: node.primitive,
                model,
                base_color,
                is_selected,
                dist_to_camera: dist,
                is_transparent,
            });
        }

        jobs.sort_by(|a, b| match (a.is_transparent, b.is_transparent) {
            (false, true) => std::cmp::Ordering::Less,
            (true, false) => std::cmp::Ordering::Greater,
            (false, false) => a
                .dist_to_camera
                .partial_cmp(&b.dist_to_camera)
                .unwrap_or(std::cmp::Ordering::Equal),
            (true, true) => b
                .dist_to_camera
                .partial_cmp(&a.dist_to_camera)
                .unwrap_or(std::cmp::Ordering::Equal),
        });

        let use_tonality =
            !(matches!(options.mode, RenderMode::Solid) && !options.solid_face_tonality);
        let mut grid_drawn = false;
        for job in &jobs {
            if job.is_transparent && !grid_drawn {
                if options.show_grid_3d && matches!(camera.mode, CameraMode::Perspective) {
                    record_world_grid(
                        &mut commands,
                        camera,
                        options.grid_spacing,
                        options.grid_load_distance,
                        options.grid_y,
                        options.grid_no_depth_test,
                        grid_bounds,
                    );
                }
                grid_drawn = true;
            }

            let override_mesh = mesh_override.and_then(|(override_id, override_mesh)| {
                (override_id == job.id).then_some(override_mesh)
            });
            let override_edges = override_mesh.map(primitives::extract_edges);
            let override_basic_mesh = override_mesh.map(|mesh| Arc::new(mesh_to_basic(mesh)));

            let (mesh, basic_mesh, edges): (&MeshData, Arc<BasicMesh>, &[[Vec3; 2]]) =
                if let Some(override_mesh) = override_mesh {
                    (
                        override_mesh,
                        override_basic_mesh
                            .unwrap_or_else(|| Arc::new(mesh_to_basic(override_mesh))),
                        override_edges.as_deref().unwrap_or(&[]),
                    )
                } else {
                    match job.primitive {
                        Primitive::Cube => (cube_mesh, Arc::clone(&cube_basic_mesh), cube_edges),
                        Primitive::Cylinder => (
                            cylinder_mesh,
                            Arc::clone(&cylinder_basic_mesh),
                            cylinder_edges,
                        ),
                        Primitive::Sphere => {
                            (sphere_mesh, Arc::clone(&sphere_basic_mesh), sphere_edges)
                        }
                        Primitive::Plane => {
                            (plane_mesh, Arc::clone(&plane_basic_mesh), plane_edges)
                        }
                        _ => (cube_mesh, Arc::clone(&cube_basic_mesh), cube_edges),
                    }
                };

            let color = if job.is_selected {
                [
                    job.base_color[0].saturating_add(30),
                    job.base_color[1].saturating_add(30),
                    job.base_color[2].saturating_add(30),
                    job.base_color[3],
                ]
            } else {
                job.base_color
            };

            if !matches!(options.mode, RenderMode::Wireframe) {
                commands.set_pipeline(if use_tonality {
                    BasicPipelineKind::PbrLit
                } else {
                    BasicPipelineKind::FlatColor
                });
                let mesh_id = commands.register_mesh(basic_mesh);
                commands.draw_mesh(mesh_id, job.model, color);
                stats.triangles_rendered += mesh.triangle_count() as u32;
            }

            let draw_surface_edges = match options.mode {
                RenderMode::Wireframe => true,
                RenderMode::Preview => true,
                RenderMode::Solid => options.solid_show_surface_edges,
            };

            if draw_surface_edges || (options.selection_outline && job.is_selected) {
                let edge_color = if job.is_selected && options.selection_outline {
                    let is_primary = options.primary_selected == Some(job.id.0 as u64);
                    if is_primary {
                        options.selection_outline_color
                    } else {
                        options.secondary_selection_outline_color
                    }
                } else {
                    surface_edge_color(job.base_color)
                };
                record_wireframe_overlay(&mut commands, edges, &job.model, edge_color);
            }
        }

        if !grid_drawn {
            if options.show_grid_3d && matches!(camera.mode, CameraMode::Perspective) {
                record_world_grid(
                    &mut commands,
                    camera,
                    options.grid_spacing,
                    options.grid_load_distance,
                    options.grid_y,
                    options.grid_no_depth_test,
                    grid_bounds,
                );
            }
        }

        self.stats = stats.clone();

        SceneRenderFrame {
            commands,
            view_proj: vp,
            light_dir,
            width: w,
            height: h,
            stats,
        }
    }

    /// Get the framebuffer dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.framebuffer.width(), self.framebuffer.height())
    }
}

pub(crate) fn rasterize_basic_scene_frame(frame: &SceneRenderFrame, framebuffer: &mut Framebuffer) {
    framebuffer.resize(frame.width, frame.height);
    let vp_w = frame.width as f32;
    let vp_h = frame.height as f32;
    let mut current_pipeline = BasicPipelineKind::FlatColor;

    for command in frame.commands.commands().iter().cloned() {
        match command {
            GraphicCommand::Clear { r, g, b, a } => framebuffer.clear(r, g, b, a),
            GraphicCommand::SetPipeline(pipeline) => current_pipeline = pipeline,
            GraphicCommand::DrawMesh {
                mesh_id,
                transform,
                color,
            } => {
                let Some(mesh) = frame.commands.mesh(mesh_id) else {
                    continue;
                };
                rasterize_mesh_command(
                    framebuffer,
                    mesh,
                    &frame.view_proj,
                    &transform,
                    frame.light_dir,
                    color,
                    current_pipeline,
                    vp_w,
                    vp_h,
                );
            }
            GraphicCommand::DrawLine {
                start,
                end,
                color,
                no_depth_test,
                depth_bias,
                ..
            } => {
                rasterize_world_line_command(
                    framebuffer,
                    &frame.view_proj,
                    start,
                    end,
                    vp_w,
                    vp_h,
                    color,
                    depth_bias,
                    no_depth_test,
                );
            }
            GraphicCommand::DrawGrid { .. } => {}
        }
    }
}

/// Intermediate struct to decouple scene traversal from framebuffer mutation.
struct RenderJob {
    id: SceneNodeId,
    primitive: Primitive,
    model: Mat4,
    base_color: [u8; 4],
    is_selected: bool,
    dist_to_camera: f32,
    is_transparent: bool,
}

fn mesh_to_basic(mesh: &MeshData) -> BasicMesh {
    BasicMesh::new(
        mesh.positions
            .iter()
            .enumerate()
            .map(
                |(index, position)| crate::api_graphic_basic::mesh::BasicVertex {
                    position: *position,
                    normal: mesh.normals.get(index).copied().unwrap_or(Vec3::Y),
                    uv: [0.0, 0.0],
                },
            )
            .collect(),
        mesh.indices.clone(),
    )
}

fn record_wireframe_overlay(
    commands: &mut BasicCommandList,
    edges: &[[Vec3; 2]],
    model: &Mat4,
    color: [u8; 4],
) {
    for edge in edges {
        let start = (*model * edge[0].extend(1.0)).truncate();
        let end = (*model * edge[1].extend(1.0)).truncate();
        commands.draw_line(start, end, color, 1.0, false, -0.001);
    }
}

fn record_world_grid(
    commands: &mut BasicCommandList,
    camera: &Camera,
    spacing: f32,
    load_distance: f32,
    grid_y: f32,
    no_depth_test: bool,
    bounds: Option<GridBounds>,
) {
    let base_spacing = spacing.max(0.25);
    let margin = load_distance.max(base_spacing);
    let bounds = bounds.unwrap_or(GridBounds {
        min_x: -margin,
        max_x: margin,
        min_z: -margin,
        max_z: margin,
    });
    let center_x = (bounds.min_x + bounds.max_x) * 0.5;
    let center_z = (bounds.min_z + bounds.max_z) * 0.5;
    let half_x = ((bounds.max_x - bounds.min_x) * 0.5).min(margin) + margin;
    let half_z = ((bounds.max_z - bounds.min_z) * 0.5).min(margin) + margin;
    let min_x = ((center_x - half_x) / base_spacing).floor() * base_spacing;
    let max_x = ((center_x + half_x) / base_spacing).ceil() * base_spacing;
    let min_z = ((center_z - half_z) / base_spacing).floor() * base_spacing;
    let max_z = ((center_z + half_z) / base_spacing).ceil() * base_spacing;
    let bounds_min = Vec3::new(min_x, 0.0, min_z);
    let bounds_max = Vec3::new(max_x, 0.0, max_z);
    let cam_y = camera.eye().y.abs();
    let depth_bias = (cam_y * 0.0005 + 0.002).clamp(0.001, 0.02);

    for line in build_3d_grid(bounds_min, bounds_max, base_spacing) {
        let color = match line.kind {
            GridLineKind::Axis => [240, 146, 36, 255],
            GridLineKind::Major => [200, 200, 206, 255],
            GridLineKind::Minor => [224, 224, 228, 255],
        };

        let start = Vec3::new(line.start.x, grid_y, line.start.z);
        let end = Vec3::new(line.end.x, grid_y, line.end.z);
        commands.draw_line(start, end, color, 1.0, no_depth_test, depth_bias);
    }
}

fn rasterize_mesh_command(
    fb: &mut Framebuffer,
    mesh: &BasicMesh,
    view_proj: &Mat4,
    model: &Mat4,
    light_dir: Vec3,
    color: [u8; 4],
    pipeline: BasicPipelineKind,
    vp_w: f32,
    vp_h: f32,
) {
    let mvp = *view_proj * *model;
    let normal_mat = transform::normal_matrix(model);
    let shaded = matches!(pipeline, BasicPipelineKind::PbrLit);

    for tri_idx in (0..mesh.indices.len()).step_by(3) {
        let i0 = mesh.indices[tri_idx] as usize;
        let i1 = mesh.indices[tri_idx + 1] as usize;
        let i2 = mesh.indices[tri_idx + 2] as usize;

        let p0 = mesh.vertices[i0].position;
        let p1 = mesh.vertices[i1].position;
        let p2 = mesh.vertices[i2].position;

        let c0 = mvp * Vec4::new(p0.x, p0.y, p0.z, 1.0);
        let c1 = mvp * Vec4::new(p1.x, p1.y, p1.z, 1.0);
        let c2 = mvp * Vec4::new(p2.x, p2.y, p2.z, 1.0);

        if c0.w <= 0.001 || c1.w <= 0.001 || c2.w <= 0.001 {
            continue;
        }

        let shade0 = if shaded {
            0.3 + 0.7
                * transform::transform_normal(mesh.vertices[i0].normal, &normal_mat)
                    .dot(light_dir)
                    .max(0.0)
        } else {
            1.0
        };
        let shade1 = if shaded {
            0.3 + 0.7
                * transform::transform_normal(mesh.vertices[i1].normal, &normal_mat)
                    .dot(light_dir)
                    .max(0.0)
        } else {
            1.0
        };
        let shade2 = if shaded {
            0.3 + 0.7
                * transform::transform_normal(mesh.vertices[i2].normal, &normal_mat)
                    .dot(light_dir)
                    .max(0.0)
        } else {
            1.0
        };

        let to_screen = |c: Vec4, shade: f32| -> ScreenVertex {
            let inv_w = 1.0 / c.w;
            let ndc_x = c.x * inv_w;
            let ndc_y = c.y * inv_w;
            let ndc_z = c.z * inv_w;
            ScreenVertex {
                x: (ndc_x + 1.0) * 0.5 * vp_w,
                y: (1.0 - ndc_y) * 0.5 * vp_h,
                z: (ndc_z + 1.0) * 0.5,
                shade,
            }
        };

        let sv0 = to_screen(c0, shade0);
        let sv1 = to_screen(c1, shade1);
        let sv2 = to_screen(c2, shade2);

        if color[3] < 255 {
            rasterizer::rasterize_triangle_blended(
                fb, sv0, sv1, sv2, color[0], color[1], color[2], color[3],
            );
        } else {
            rasterizer::rasterize_triangle(
                fb, sv0, sv1, sv2, color[0], color[1], color[2], color[3],
            );
        }
    }
}

fn rasterize_world_line_command(
    fb: &mut Framebuffer,
    view_proj: &Mat4,
    start: Vec3,
    end: Vec3,
    vp_w: f32,
    vp_h: f32,
    color: [u8; 4],
    depth_bias: f32,
    no_depth_test: bool,
) {
    let c0 = *view_proj * start.extend(1.0);
    let c1 = *view_proj * end.extend(1.0);

    if line_outside_clip(c0, c1) {
        return;
    }

    let (c0, c1) = match clip_line_near(c0, c1) {
        Some(clipped) => clipped,
        None => return,
    };

    let x0 = (c0.x / c0.w + 1.0) * 0.5 * vp_w;
    let y0 = (1.0 - c0.y / c0.w) * 0.5 * vp_h;
    let z0 = ((c0.z / c0.w + 1.0) * 0.5 + depth_bias).min(0.9995);

    let x1 = (c1.x / c1.w + 1.0) * 0.5 * vp_w;
    let y1 = (1.0 - c1.y / c1.w) * 0.5 * vp_h;
    let z1 = ((c1.z / c1.w + 1.0) * 0.5 + depth_bias).min(0.9995);

    if no_depth_test {
        rasterizer::rasterize_line_no_depth(
            fb, x0, y0, x1, y1, color[0], color[1], color[2], color[3],
        );
    } else {
        rasterizer::rasterize_line(
            fb, x0, y0, z0, x1, y1, z1, color[0], color[1], color[2], color[3],
        );
    }
}

/// Draw wireframe edges for a selected object (free function to avoid borrow conflict).
fn draw_wireframe_overlay(
    fb: &mut Framebuffer,
    edges: &[[Vec3; 2]],
    mvp: &Mat4,
    vp_w: f32,
    vp_h: f32,
    color: [u8; 4],
) {
    for edge in edges {
        let c0_raw = *mvp * Vec4::new(edge[0].x, edge[0].y, edge[0].z, 1.0);
        let c1_raw = *mvp * Vec4::new(edge[1].x, edge[1].y, edge[1].z, 1.0);

        if line_outside_clip(c0_raw, c1_raw) {
            continue;
        }

        let (c0, c1) = match clip_line_near(c0_raw, c1_raw) {
            Some(clipped) => clipped,
            None => continue,
        };

        let x0 = (c0.x / c0.w + 1.0) * 0.5 * vp_w;
        let y0 = (1.0 - c0.y / c0.w) * 0.5 * vp_h;
        let z0 = (c0.z / c0.w + 1.0) * 0.5 - 0.001;

        let x1 = (c1.x / c1.w + 1.0) * 0.5 * vp_w;
        let y1 = (1.0 - c1.y / c1.w) * 0.5 * vp_h;
        let z1 = (c1.z / c1.w + 1.0) * 0.5 - 0.001;

        rasterizer::rasterize_line(
            fb, x0, y0, z0, x1, y1, z1, color[0], color[1], color[2], color[3],
        );
    }
}

fn surface_edge_color(base_color: [u8; 4]) -> [u8; 4] {
    [
        base_color[0].saturating_sub(70),
        base_color[1].saturating_sub(70),
        base_color[2].saturating_sub(70),
        255,
    ]
}

fn draw_world_grid(
    fb: &mut Framebuffer,
    camera: &Camera,
    vp_w: f32,
    vp_h: f32,
    spacing: f32,
    load_distance: f32,
    bounds: Option<GridBounds>,
) {
    let base_spacing = spacing.max(0.25);
    let margin = load_distance.max(base_spacing);
    let bounds = bounds.unwrap_or(GridBounds {
        min_x: -margin,
        max_x: margin,
        min_z: -margin,
        max_z: margin,
    });
    let center_x = (bounds.min_x + bounds.max_x) * 0.5;
    let center_z = (bounds.min_z + bounds.max_z) * 0.5;
    let half_x = ((bounds.max_x - bounds.min_x) * 0.5).min(margin) + margin;
    let half_z = ((bounds.max_z - bounds.min_z) * 0.5).min(margin) + margin;
    let min_x = ((center_x - half_x) / base_spacing).floor() * base_spacing;
    let max_x = ((center_x + half_x) / base_spacing).ceil() * base_spacing;
    let min_z = ((center_z - half_z) / base_spacing).floor() * base_spacing;
    let max_z = ((center_z + half_z) / base_spacing).ceil() * base_spacing;
    let bounds_min = Vec3::new(min_x, 0.0, min_z);
    let bounds_max = Vec3::new(max_x, 0.0, max_z);
    let view_proj = camera.view_projection(vp_w, vp_h);
    let cam_y = camera.eye().y.abs();
    let depth_bias = (cam_y * 0.0005 + 0.002).clamp(0.001, 0.02);

    for line in build_3d_grid(bounds_min, bounds_max, base_spacing) {
        let color = match line.kind {
            GridLineKind::Axis => [240, 146, 36, 255],
            GridLineKind::Major => [200, 200, 206, 255],
            GridLineKind::Minor => [224, 224, 228, 255],
        };

        let start = Vec3::new(line.start.x, -0.02, line.start.z);
        let end = Vec3::new(line.end.x, -0.02, line.end.z);
        draw_world_line(fb, &view_proj, start, end, vp_w, vp_h, color, depth_bias);
    }
}

#[inline]
fn clip_line_near(c0: Vec4, c1: Vec4) -> Option<(Vec4, Vec4)> {
    const NEAR_W: f32 = 0.001;

    let behind0 = c0.w < NEAR_W;
    let behind1 = c1.w < NEAR_W;

    if behind0 && behind1 {
        return None;
    }

    if !behind0 && !behind1 {
        return Some((c0, c1));
    }

    let denom = c1.w - c0.w;
    if denom.abs() <= f32::EPSILON {
        return None;
    }

    let t = ((NEAR_W - c0.w) / denom).clamp(0.0, 1.0);
    let clipped = c0 + (c1 - c0) * t;

    if behind0 {
        Some((clipped, c1))
    } else {
        Some((c0, clipped))
    }
}

fn draw_world_line(
    fb: &mut Framebuffer,
    view_proj: &Mat4,
    start: Vec3,
    end: Vec3,
    vp_w: f32,
    vp_h: f32,
    color: [u8; 4],
    depth_bias: f32,
) {
    let c0 = *view_proj * start.extend(1.0);
    let c1 = *view_proj * end.extend(1.0);

    if line_outside_clip(c0, c1) {
        return;
    }

    let (c0, c1) = match clip_line_near(c0, c1) {
        Some(clipped) => clipped,
        None => return,
    };

    let x0 = (c0.x / c0.w + 1.0) * 0.5 * vp_w;
    let y0 = (1.0 - c0.y / c0.w) * 0.5 * vp_h;
    let z0 = ((c0.z / c0.w + 1.0) * 0.5 + depth_bias).min(0.9995);

    let x1 = (c1.x / c1.w + 1.0) * 0.5 * vp_w;
    let y1 = (1.0 - c1.y / c1.w) * 0.5 * vp_h;
    let z1 = ((c1.z / c1.w + 1.0) * 0.5 + depth_bias).min(0.9995);

    rasterizer::rasterize_line(
        fb, x0, y0, z0, x1, y1, z1, color[0], color[1], color[2], color[3],
    );
}

fn line_outside_clip(c0: Vec4, c1: Vec4) -> bool {
    (c0.x < -c0.w && c1.x < -c1.w)
        || (c0.x > c0.w && c1.x > c1.w)
        || (c0.y < -c0.w && c1.y < -c1.w)
        || (c0.y > c0.w && c1.y > c1.w)
        || (c0.z < -c0.w && c1.z < -c1.w)
        || (c0.z > c0.w && c1.z > c1.w)
}
