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
//! This module lives in raf_render and has no UI dependency.
//! The editor's viewport host consumes its scene output directly.

use glam::{Mat4, Vec3, Vec4};
use std::collections::HashSet;
use std::sync::Arc;

use crate::api_graphic_basic::command_list::{BasicCommandList, BasicLine, GraphicCommand};
use crate::api_graphic_basic::grid::{build_3d_grid, GridLineKind};
use crate::api_graphic_basic::mesh::BasicMesh;
use crate::api_graphic_basic::pipeline::BasicPipelineKind;
use crate::camera::{Camera, CameraMode};
use crate::geometry::mesh_data::MeshData;
use crate::geometry::primitives;
use crate::math::clip::{clip_triangle_to_near, ClipVertex};
use crate::math::frustum::Frustum;
use crate::math::transform;
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::render_pipeline::rasterizer::{self, ScreenVertex};
use crate::scene_visibility::{
    SceneObjectBounds, SceneVisibility, SceneVisibilityPolicy, WorldStreamVisibility,
};

use raf_core::scene::graph::{Primitive, SceneGraph, SceneNodeId};
use raf_core::scene::WorldTransformCache;

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
    /// Entities fully outside the camera frustum.
    pub frustum_culled_entities: u32,
    /// Entities outside the active streamed camera region.
    pub streaming_culled_entities: u32,
}

/// Per-frame renderer options supplied by the editor viewport.
///
/// These values keep the scene renderer independent from the UI while still
/// allowing the editor to control presentation details such as solid edges,
/// xray opacity, and selection highlighting.
#[derive(Debug, Clone, Copy)]
pub struct RenderOptions {
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
    /// Enable region-based visibility before frustum culling and draw sorting.
    pub world_streaming_enabled: bool,
    /// Edge length in meters for a streamed world region.
    pub world_stream_region_size: f32,
    /// Visible region radius around the camera.
    pub world_stream_load_radius: u32,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
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
            world_streaming_enabled: false,
            world_stream_region_size: 128.0,
            world_stream_load_radius: 3,
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
    cube_local_bounds: (Vec3, Vec3),
    cylinder_mesh: MeshData,
    cylinder_basic_mesh: Arc<BasicMesh>,
    cylinder_edges: Vec<[Vec3; 2]>,
    cylinder_local_bounds: (Vec3, Vec3),
    sphere_mesh: MeshData,
    sphere_basic_mesh: Arc<BasicMesh>,
    sphere_edges: Vec<[Vec3; 2]>,
    sphere_local_bounds: (Vec3, Vec3),
    plane_mesh: MeshData,
    plane_basic_mesh: Arc<BasicMesh>,
    plane_edges: Vec<[Vec3; 2]>,
    plane_local_bounds: (Vec3, Vec3),
    world_transforms: Option<(u64, WorldTransformCache)>,
    render_jobs: Vec<RenderJob>,
    selected_ids: HashSet<SceneNodeId>,
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

/// Fixed height of the world grid plane. The grid is intentionally
/// detached from both the camera and the scene content: it never follows
/// the camera height, never stretches to cover the scene bounds, and never
/// rides along an object being dragged. It stays flat and quiet.
pub const GRID_Y: f32 = -0.02;

impl SceneRenderer {
    /// Create a new renderer with initial viewport dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let cube_mesh = primitives::cube(1);
        let cylinder_mesh = primitives::cylinder(32);
        let sphere_mesh = primitives::sphere(16, 24);
        let plane_mesh = primitives::plane(1);
        let cube_local_bounds = cube_mesh.aabb();
        let cylinder_local_bounds = cylinder_mesh.aabb();
        let sphere_local_bounds = sphere_mesh.aabb();
        let plane_local_bounds = plane_mesh.aabb();

        Self {
            framebuffer: Framebuffer::new(width.max(1), height.max(1)),
            cube_basic_mesh: Arc::new(mesh_to_basic(&cube_mesh)),
            cube_edges: primitives::extract_edges(&cube_mesh),
            cube_local_bounds,
            cube_mesh,
            cylinder_basic_mesh: Arc::new(mesh_to_basic(&cylinder_mesh)),
            cylinder_edges: primitives::extract_edges(&cylinder_mesh),
            cylinder_local_bounds,
            cylinder_mesh,
            sphere_basic_mesh: Arc::new(mesh_to_basic(&sphere_mesh)),
            sphere_edges: primitives::extract_edges(&sphere_mesh),
            sphere_local_bounds,
            sphere_mesh,
            plane_basic_mesh: Arc::new(mesh_to_basic(&plane_mesh)),
            plane_edges: primitives::extract_edges(&plane_mesh),
            plane_local_bounds,
            plane_mesh,
            world_transforms: None,
            render_jobs: Vec::new(),
            selected_ids: HashSet::new(),
            stats: FrameStats::default(),
        }
    }

    fn local_bounds_for_primitive(&self, primitive: Primitive) -> (Vec3, Vec3) {
        match primitive {
            Primitive::Cube => self.cube_local_bounds,
            Primitive::Cylinder => self.cylinder_local_bounds,
            Primitive::Sphere => self.sphere_local_bounds,
            Primitive::Plane => self.plane_local_bounds,
            Primitive::Empty => self.cube_local_bounds,
        }
    }

    fn collect_render_jobs(
        &mut self,
        scene: &SceneGraph,
        frustum: &Frustum,
        camera_position: Vec3,
        selected: &[SceneNodeId],
        options: RenderOptions,
        mesh_override: Option<(SceneNodeId, &MeshData)>,
    ) -> (Vec<RenderJob>, FrameStats) {
        self.selected_ids.clear();
        self.selected_ids.extend(selected.iter().copied());
        let scene_revision = scene.document_revision();
        if self
            .world_transforms
            .as_ref()
            .is_none_or(|(revision, _)| *revision != scene_revision)
        {
            self.world_transforms = Some((scene_revision, WorldTransformCache::build(scene)));
        }
        let transforms = &self
            .world_transforms
            .as_ref()
            .expect("world transforms were initialized for the scene")
            .1;
        let visibility = SceneVisibilityPolicy {
            world_stream: WorldStreamVisibility {
                enabled: options.world_streaming_enabled,
                region_size: options.world_stream_region_size,
                load_radius: options.world_stream_load_radius,
            },
        };
        let mut jobs = std::mem::take(&mut self.render_jobs);
        jobs.clear();
        jobs.reserve(scene.len());
        let mut stats = FrameStats::default();
        let override_bounds = mesh_override.map(|(override_id, mesh)| (override_id, mesh.aabb()));

        for (id, node) in scene.iter() {
            if !node.visible || matches!(node.primitive, Primitive::Empty) {
                continue;
            }
            stats.total_entities += 1;

            let model = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            let (local_min, local_max) = override_bounds
                .and_then(|(override_id, bounds)| (override_id == id).then_some(bounds))
                .unwrap_or_else(|| self.local_bounds_for_primitive(node.primitive));
            let bounds = SceneObjectBounds::from_local_aabb(local_min, local_max, model);

            match visibility.classify(frustum, camera_position, bounds) {
                SceneVisibility::Visible => {}
                SceneVisibility::OutsideFrustum => {
                    stats.frustum_culled_entities += 1;
                    continue;
                }
                SceneVisibility::OutsideStream => {
                    stats.streaming_culled_entities += 1;
                    continue;
                }
            }

            stats.visible_entities += 1;
            let mut base_color = [node.color.r, node.color.g, node.color.b, node.color.a];
            if options.solid_xray_mode {
                base_color[3] = base_color[3].min(120);
            }
            let center = bounds.center();

            jobs.push(RenderJob {
                id,
                primitive: node.primitive,
                model,
                base_color,
                is_selected: self.selected_ids.contains(&id),
                distance_squared: (center - camera_position).length_squared(),
                is_transparent: base_color[3] < u8::MAX,
            });
        }

        let max_opaque_distance_squared = jobs
            .iter()
            .filter(|job| !job.is_transparent)
            .map(|job| job.distance_squared)
            .fold(0.0_f32, f32::max);

        // Opaque geometry keeps coarse front-to-back buckets for early depth
        // rejection and groups identical primitive meshes inside each bucket.
        // That preserves depth correctness while allowing the command list to
        // turn repeated meshes into one instanced draw. Transparent geometry
        // remains strictly back-to-front.
        jobs.sort_by(|a, b| {
            let order = match (a.is_transparent, b.is_transparent) {
                (false, true) => std::cmp::Ordering::Less,
                (true, false) => std::cmp::Ordering::Greater,
                (false, false) => {
                    opaque_depth_bucket(a.distance_squared, max_opaque_distance_squared)
                        .cmp(&opaque_depth_bucket(
                            b.distance_squared,
                            max_opaque_distance_squared,
                        ))
                        .then_with(|| {
                            primitive_batch_key(a.primitive).cmp(&primitive_batch_key(b.primitive))
                        })
                        .then_with(|| {
                            a.distance_squared
                                .partial_cmp(&b.distance_squared)
                                .unwrap_or(std::cmp::Ordering::Equal)
                        })
                }
                (true, true) => b
                    .distance_squared
                    .partial_cmp(&a.distance_squared)
                    .unwrap_or(std::cmp::Ordering::Equal),
            };
            order.then_with(|| a.id.0.cmp(&b.id.0))
        });

        (jobs, stats)
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
        let (jobs, mut stats) =
            self.collect_render_jobs(scene, &frustum, cam_eye, selected, options, mesh_override);

        let cube_mesh = &self.cube_mesh;
        let cube_edges = self.cube_edges.as_slice();
        let cylinder_mesh = &self.cylinder_mesh;
        let cylinder_edges = self.cylinder_edges.as_slice();
        let sphere_mesh = &self.sphere_mesh;
        let sphere_edges = self.sphere_edges.as_slice();
        let plane_mesh = &self.plane_mesh;
        let plane_edges = self.plane_edges.as_slice();
        if options.show_grid_3d && matches!(camera.mode, CameraMode::Perspective) {
            draw_world_grid(
                &mut self.framebuffer,
                camera,
                vp_w,
                vp_h,
                options.grid_spacing,
                options.grid_load_distance,
                options.grid_y,
            );
        }

        // Execute render jobs (now we can borrow framebuffer mutably)
        let use_tonality = options.solid_face_tonality;
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
                let rendered = rasterize_clipped_triangle(
                    &mut self.framebuffer,
                    [
                        ClipVertex {
                            position: c0,
                            shade: shade0,
                        },
                        ClipVertex {
                            position: c1,
                            shade: shade1,
                        },
                        ClipVertex {
                            position: c2,
                            shade: shade2,
                        },
                    ],
                    color,
                    vp_w,
                    vp_h,
                );
                if rendered == 0 {
                    stats.triangles_culled += 1;
                } else {
                    stats.triangles_rendered += rendered;
                }
            }

            let draw_surface_edges = options.solid_show_surface_edges;

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

        self.render_jobs = jobs;
        self.render_jobs.clear();
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
        let (jobs, mut stats) =
            self.collect_render_jobs(scene, &frustum, cam_eye, selected, options, mesh_override);

        let cube_mesh = &self.cube_mesh;
        let cube_basic_mesh = Arc::clone(&self.cube_basic_mesh);
        let cube_edges = self.cube_edges.as_slice();
        let cylinder_mesh = &self.cylinder_mesh;
        let cylinder_basic_mesh = Arc::clone(&self.cylinder_basic_mesh);
        let cylinder_edges = self.cylinder_edges.as_slice();
        let sphere_mesh = &self.sphere_mesh;
        let sphere_basic_mesh = Arc::clone(&self.sphere_basic_mesh);
        let sphere_edges = self.sphere_edges.as_slice();
        let plane_mesh = &self.plane_mesh;
        let plane_basic_mesh = Arc::clone(&self.plane_basic_mesh);
        let plane_edges = self.plane_edges.as_slice();
        let override_edges = mesh_override.map(|(_, mesh)| primitives::extract_edges(mesh));
        let override_basic_mesh = mesh_override.map(|(_, mesh)| Arc::new(mesh_to_basic(mesh)));
        let mut commands = BasicCommandList::with_capacity(
            jobs.len().saturating_add(6),
            4 + usize::from(override_basic_mesh.is_some()),
        );
        commands.clear(bg_color);

        let use_tonality = options.solid_face_tonality;
        let mut grid_drawn = false;
        for job in &jobs {
            if job.is_transparent && !grid_drawn {
                if options.show_grid_3d && matches!(camera.mode, CameraMode::Perspective) {
                    record_world_grid(
                        &mut commands,
                        options.grid_spacing,
                        options.grid_load_distance,
                        options.grid_y,
                        options.grid_no_depth_test,
                    );
                }
                grid_drawn = true;
            }

            let job_override_mesh = mesh_override.and_then(|(override_id, override_mesh)| {
                (override_id == job.id).then_some(override_mesh)
            });

            let (mesh, basic_mesh): (&MeshData, Arc<BasicMesh>) =
                if let Some(job_override_mesh) = job_override_mesh {
                    (
                        job_override_mesh,
                        override_basic_mesh
                            .as_ref()
                            .map(Arc::clone)
                            .unwrap_or_else(|| Arc::new(mesh_to_basic(job_override_mesh))),
                    )
                } else {
                    match job.primitive {
                        Primitive::Cube => (cube_mesh, Arc::clone(&cube_basic_mesh)),
                        Primitive::Cylinder => (cylinder_mesh, Arc::clone(&cylinder_basic_mesh)),
                        Primitive::Sphere => (sphere_mesh, Arc::clone(&sphere_basic_mesh)),
                        Primitive::Plane => (plane_mesh, Arc::clone(&plane_basic_mesh)),
                        _ => (cube_mesh, Arc::clone(&cube_basic_mesh)),
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

            commands.set_pipeline(if use_tonality {
                BasicPipelineKind::PbrLit
            } else {
                BasicPipelineKind::FlatColor
            });
            let mesh_id = if job_override_mesh.is_some() {
                commands.register_transient_mesh(basic_mesh)
            } else {
                commands.register_mesh(basic_mesh)
            };
            commands.draw_mesh(mesh_id, job.model, color);
            stats.triangles_rendered += mesh.triangle_count() as u32;
        }

        // Edges are intentionally recorded after all mesh commands. Interleaving
        // one line command per object would split otherwise compatible mesh
        // runs and defeat instancing when surface edges are enabled.
        for job in &jobs {
            let draw_surface_edges = options.solid_show_surface_edges;
            if draw_surface_edges || (options.selection_outline && job.is_selected) {
                let edges = if mesh_override.is_some_and(|(override_id, _)| override_id == job.id) {
                    override_edges.as_deref().unwrap_or(&[])
                } else {
                    match job.primitive {
                        Primitive::Cube => cube_edges,
                        Primitive::Cylinder => cylinder_edges,
                        Primitive::Sphere => sphere_edges,
                        Primitive::Plane => plane_edges,
                        Primitive::Empty => &[],
                    }
                };
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
                    options.grid_spacing,
                    options.grid_load_distance,
                    options.grid_y,
                    options.grid_no_depth_test,
                );
            }
        }

        self.render_jobs = jobs;
        self.render_jobs.clear();
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

    // Keep the frame command list borrowed; cloning a line batch here would
    // allocate on every CPU fallback frame.
    for command in frame.commands.commands() {
        match command {
            GraphicCommand::Clear { r, g, b, a } => framebuffer.clear(*r, *g, *b, *a),
            GraphicCommand::SetPipeline(pipeline) => current_pipeline = *pipeline,
            GraphicCommand::DrawMesh {
                mesh_id,
                transform,
                color,
            } => {
                let Some(mesh) = frame.commands.mesh(*mesh_id) else {
                    continue;
                };
                rasterize_mesh_command(
                    framebuffer,
                    mesh,
                    &frame.view_proj,
                    transform,
                    frame.light_dir,
                    *color,
                    current_pipeline,
                    vp_w,
                    vp_h,
                );
            }
            GraphicCommand::DrawMeshBatch { mesh_id, instances } => {
                let Some(mesh) = frame.commands.mesh(*mesh_id) else {
                    continue;
                };
                for instance in instances {
                    rasterize_mesh_command(
                        framebuffer,
                        mesh,
                        &frame.view_proj,
                        &instance.transform,
                        frame.light_dir,
                        instance.color,
                        current_pipeline,
                        vp_w,
                        vp_h,
                    );
                }
            }
            GraphicCommand::DrawLine {
                start,
                end,
                color,
                width,
                no_depth_test,
                depth_bias,
            } => {
                rasterize_world_line_command(
                    framebuffer,
                    &frame.view_proj,
                    *start,
                    *end,
                    vp_w,
                    vp_h,
                    *color,
                    *width,
                    *depth_bias,
                    *no_depth_test,
                );
            }
            GraphicCommand::DrawLineBatch {
                lines,
                no_depth_test,
            } => {
                for line in lines {
                    rasterize_world_line_command(
                        framebuffer,
                        &frame.view_proj,
                        line.start,
                        line.end,
                        vp_w,
                        vp_h,
                        line.color,
                        line.width,
                        line.depth_bias,
                        *no_depth_test,
                    );
                }
            }
            GraphicCommand::DrawScreenTriangleBatch { triangles } => {
                for triangle in triangles {
                    rasterize_screen_triangle_no_depth(
                        framebuffer,
                        triangle.points,
                        triangle.color,
                    );
                }
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
    distance_squared: f32,
    is_transparent: bool,
}

const OPAQUE_DEPTH_BUCKET_COUNT: f32 = 32.0;

#[inline]
fn opaque_depth_bucket(distance_squared: f32, max_distance_squared: f32) -> u8 {
    if !distance_squared.is_finite() || max_distance_squared <= f32::EPSILON {
        return 0;
    }
    ((distance_squared.max(0.0) / max_distance_squared) * (OPAQUE_DEPTH_BUCKET_COUNT - 1.0))
        .floor()
        .clamp(0.0, OPAQUE_DEPTH_BUCKET_COUNT - 1.0) as u8
}

#[inline]
const fn primitive_batch_key(primitive: Primitive) -> u8 {
    match primitive {
        Primitive::Cube => 0,
        Primitive::Cylinder => 1,
        Primitive::Sphere => 2,
        Primitive::Plane => 3,
        Primitive::Empty => 4,
    }
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
    commands.draw_line_batch(
        edges.iter().map(|edge| BasicLine {
            start: (*model * edge[0].extend(1.0)).truncate(),
            end: (*model * edge[1].extend(1.0)).truncate(),
            color,
            width: 1.0,
            depth_bias: -0.001,
        }),
        false,
    );
}

fn record_world_grid(
    commands: &mut BasicCommandList,
    spacing: f32,
    load_distance: f32,
    grid_y: f32,
    no_depth_test: bool,
) {
    const DEPTH_BIAS: f32 = 0.002;

    let base_spacing = spacing.max(0.25);
    let margin = load_distance.max(base_spacing);
    let min_x = (-margin / base_spacing).floor() * base_spacing;
    let max_x = (margin / base_spacing).ceil() * base_spacing;
    let min_z = (-margin / base_spacing).floor() * base_spacing;
    let max_z = (margin / base_spacing).ceil() * base_spacing;
    let bounds_min = Vec3::new(min_x, 0.0, min_z);
    let bounds_max = Vec3::new(max_x, 0.0, max_z);

    commands.draw_line_batch(
        build_3d_grid(bounds_min, bounds_max, base_spacing)
            .into_iter()
            .map(|line| BasicLine {
                start: Vec3::new(line.start.x, grid_y, line.start.z),
                end: Vec3::new(line.end.x, grid_y, line.end.z),
                color: match line.kind {
                    GridLineKind::Axis => [240, 146, 36, 255],
                    GridLineKind::Major => [200, 200, 206, 255],
                    GridLineKind::Minor => [224, 224, 228, 255],
                },
                width: 1.0,
                depth_bias: DEPTH_BIAS,
            }),
        no_depth_test,
    );
}

fn rasterize_clipped_triangle(
    framebuffer: &mut Framebuffer,
    triangle: [ClipVertex; 3],
    color: [u8; 4],
    vp_w: f32,
    vp_h: f32,
) -> u32 {
    let clipped = clip_triangle_to_near(triangle);
    let vertices = clipped.vertices();
    if vertices.len() < 3 {
        return 0;
    }

    let to_screen = |vertex: ClipVertex| -> Option<ScreenVertex> {
        if vertex.position.w <= 1.0e-6 {
            return None;
        }
        let inv_w = 1.0 / vertex.position.w;
        let ndc_x = vertex.position.x * inv_w;
        let ndc_y = vertex.position.y * inv_w;
        let ndc_z = vertex.position.z * inv_w;
        Some(ScreenVertex {
            x: (ndc_x + 1.0) * 0.5 * vp_w,
            y: (1.0 - ndc_y) * 0.5 * vp_h,
            z: (ndc_z + 1.0) * 0.5,
            shade: vertex.shade,
        })
    };

    let Some(first) = to_screen(vertices[0]) else {
        return 0;
    };
    let mut rendered = 0u32;
    for index in 1..vertices.len() - 1 {
        let (Some(second), Some(third)) =
            (to_screen(vertices[index]), to_screen(vertices[index + 1]))
        else {
            continue;
        };
        if color[3] < u8::MAX {
            rasterizer::rasterize_triangle_blended(
                framebuffer,
                first,
                second,
                third,
                color[0],
                color[1],
                color[2],
                color[3],
            );
        } else {
            rasterizer::rasterize_triangle(
                framebuffer,
                first,
                second,
                third,
                color[0],
                color[1],
                color[2],
                color[3],
            );
        }
        rendered += 1;
    }
    rendered
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

        rasterize_clipped_triangle(
            fb,
            [
                ClipVertex {
                    position: c0,
                    shade: shade0,
                },
                ClipVertex {
                    position: c1,
                    shade: shade1,
                },
                ClipVertex {
                    position: c2,
                    shade: shade2,
                },
            ],
            color,
            vp_w,
            vp_h,
        );
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
    width: f32,
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
        rasterizer::rasterize_line_no_depth_width(
            fb, x0, y0, x1, y1, color[0], color[1], color[2], color[3], width,
        );
    } else {
        rasterizer::rasterize_line_width(
            fb, x0, y0, z0, x1, y1, z1, color[0], color[1], color[2], color[3], width,
        );
    }
}

/// Rasterize a renderer-owned 2D overlay without touching the scene depth
/// buffer. This is the CPU counterpart of ApiGraphicBasic's overlay pipeline.
fn rasterize_screen_triangle_no_depth(fb: &mut Framebuffer, points: [[f32; 2]; 3], color: [u8; 4]) {
    let min_x = points
        .iter()
        .map(|point| point[0])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, fb.width() as f32) as u32;
    let min_y = points
        .iter()
        .map(|point| point[1])
        .fold(f32::INFINITY, f32::min)
        .floor()
        .clamp(0.0, fb.height() as f32) as u32;
    let max_x = points
        .iter()
        .map(|point| point[0])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, fb.width() as f32) as u32;
    let max_y = points
        .iter()
        .map(|point| point[1])
        .fold(f32::NEG_INFINITY, f32::max)
        .ceil()
        .clamp(0.0, fb.height() as f32) as u32;

    if min_x >= max_x || min_y >= max_y {
        return;
    }

    let edge = |a: [f32; 2], b: [f32; 2], p: [f32; 2]| {
        (p[0] - a[0]) * (b[1] - a[1]) - (p[1] - a[1]) * (b[0] - a[0])
    };
    let area = edge(points[0], points[1], points[2]);
    if area.abs() <= f32::EPSILON {
        return;
    }

    for y in min_y..max_y {
        for x in min_x..max_x {
            let point = [x as f32 + 0.5, y as f32 + 0.5];
            let w0 = edge(points[1], points[2], point);
            let w1 = edge(points[2], points[0], point);
            let w2 = edge(points[0], points[1], point);
            let inside = if area > 0.0 {
                w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0
            } else {
                w0 <= 0.0 && w1 <= 0.0 && w2 <= 0.0
            };
            if inside {
                fb.blend_pixel_no_depth(x, y, color[0], color[1], color[2], color[3]);
            }
        }
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
    grid_y: f32,
) {
    const DEPTH_BIAS: f32 = 0.002;

    let base_spacing = spacing.max(0.25);
    let margin = load_distance.max(base_spacing);
    let min_x = (-margin / base_spacing).floor() * base_spacing;
    let max_x = (margin / base_spacing).ceil() * base_spacing;
    let min_z = (-margin / base_spacing).floor() * base_spacing;
    let max_z = (margin / base_spacing).ceil() * base_spacing;
    let bounds_min = Vec3::new(min_x, 0.0, min_z);
    let bounds_max = Vec3::new(max_x, 0.0, max_z);
    let view_proj = camera.view_projection(vp_w, vp_h);

    for line in build_3d_grid(bounds_min, bounds_max, base_spacing) {
        let color = match line.kind {
            GridLineKind::Axis => [240, 146, 36, 255],
            GridLineKind::Major => [200, 200, 206, 255],
            GridLineKind::Minor => [224, 224, 228, 255],
        };

        let start = Vec3::new(line.start.x, grid_y, line.start.z);
        let end = Vec3::new(line.end.x, grid_y, line.end.z);
        draw_world_line(fb, &view_proj, start, end, vp_w, vp_h, color, DEPTH_BIAS);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_config::RenderConfig;

    #[test]
    fn lightweight_scene_submission_keeps_all_visible_geometry() {
        let mut scene = SceneGraph::new();
        for index in 0..4 {
            scene.add_root_with_primitive(&format!("Sphere {index}"), Primitive::Sphere);
        }
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);
        let expected_triangles = renderer.sphere_mesh.triangle_count() as u32 * 4;

        let frame = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::new(0.4, 1.0, 0.2),
            RenderOptions::default(),
            None,
        );

        assert!(expected_triangles > RenderConfig::editor_lightweight().max_triangles);
        assert_eq!(frame.stats.visible_entities, 4);
        assert_eq!(frame.stats.triangles_rendered, expected_triangles);
    }

    #[test]
    fn opaque_meshes_batch_inside_the_same_depth_bucket() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Cube A", Primitive::Cube);
        scene.add_root_with_primitive("Sphere", Primitive::Sphere);
        scene.add_root_with_primitive("Cube B", Primitive::Cube);
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);

        let frame = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::new(0.4, 1.0, 0.2),
            RenderOptions::default(),
            None,
        );

        assert!(frame.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawMeshBatch { instances, .. } if instances.len() == 2
        )));
    }

    #[test]
    fn camera_only_frames_reuse_world_transform_cache() {
        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);

        let _ = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions::default(),
            None,
        );
        let cached_revision = renderer.world_transforms.as_ref().unwrap().0;
        let _ = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions::default(),
            None,
        );
        assert_eq!(
            renderer.world_transforms.as_ref().unwrap().0,
            cached_revision
        );

        scene.get_mut(cube).unwrap().position = Vec3::X;
        let _ = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions::default(),
            None,
        );
        assert_ne!(
            renderer.world_transforms.as_ref().unwrap().0,
            cached_revision
        );
    }

    #[test]
    fn surface_edges_do_not_break_mesh_instancing() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Cube A", Primitive::Cube);
        scene.add_root_with_primitive("Cube B", Primitive::Cube);
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);
        let options = RenderOptions {
            show_grid_3d: false,
            solid_show_surface_edges: true,
            ..RenderOptions::default()
        };

        let frame = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            options,
            None,
        );

        assert!(matches!(
            frame.commands.commands().first(),
            Some(GraphicCommand::Clear { .. })
        ));
        assert!(frame.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawMeshBatch { instances, .. } if instances.len() == 2
        )));
        assert_eq!(
            frame
                .commands
                .commands()
                .iter()
                .filter(|command| matches!(command, GraphicCommand::DrawLineBatch { .. }))
                .count(),
            1
        );
    }
}
