//! Scene renderer: orchestrates the full render pipeline.
//!
//! Takes a scene graph, camera parameters, and viewport dimensions,
//! and produces an RGBA pixel buffer ready for display.
//!
//! Pipeline stages:
//! 1. Frustum cull (skip objects outside view)
//! 2. Select cached geometry detail per visible curved primitive
//! 3. Transform vertices: Object -> World -> Clip -> Screen
//! 4. Backface cull + clip against near plane
//! 5. Shade each triangle (flat shading)
//! 6. Rasterize with scanline + Z-buffer
//! 7. Output pixel buffer
//!
//! This module lives in raf_render and has no UI dependency.
//! The editor's viewport host consumes its scene output directly.

mod skinning;

use glam::{Mat4, Vec3, Vec4};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use crate::api_graphic_basic::asset_ingress::TextureIngress;
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
use crate::particles::{ParticleRenderOptions, ParticleRenderer};
use crate::render_pipeline::framebuffer::Framebuffer;
use crate::render_pipeline::rasterizer::{self, ScreenVertex};
use crate::scene_visibility::{
    SceneObjectBounds, SceneVisibility, SceneVisibilityPolicy, WorldStreamVisibility,
};
use crate::texture::CpuTexture;

use raf_assets::ingress::TextureDecodeLimits;
use raf_core::project::{GeometryDetailMode, TextureQualityMode};
use raf_core::scene::geometry::node_geometry_matrix;
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
    pub environment_override: Option<raf_core::environment::EnvironmentSettings>,
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
    /// Tessellation policy for built-in curved meshes.
    pub geometry_detail: GeometryDetailMode,
    /// Hard CPU/GPU cache policy for project-owned base-color textures.
    pub texture_quality: TextureQualityMode,
    /// Particle preview/runtime controls and budgets.
    pub particles: ParticleRenderOptions,
    /// Frame delta used by the particle simulation. Clamped by the renderer.
    pub particle_dt: f32,
    /// True while the editor camera is navigating. Geometry selection and LOD
    /// stay frozen so the retained scene packet can be reused instead of
    /// re-uploading every texture and mesh on each orbit/pan frame.
    pub camera_navigation: bool,
    /// Keep camera-independent command streams for moderate scenes. When set,
    /// frustum culling and camera-derived LOD are skipped so orbit/pan frames
    /// reuse the retained packet. Huge scenes keep per-frame culling.
    pub stable_scene_packet: bool,
    /// Tab Mode focus: every other scene node is dimmed instead of hidden.
    pub focus_node: Option<u64>,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            environment_override: None,
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
            geometry_detail: GeometryDetailMode::Adaptive,
            texture_quality: TextureQualityMode::Adaptive,
            particles: ParticleRenderOptions::default(),
            particle_dt: 1.0 / 60.0,
            camera_navigation: false,
            stable_scene_packet: false,
            focus_node: None,
        }
    }
}

struct MeshDetailLevel {
    mesh: MeshData,
    basic_mesh: Arc<BasicMesh>,
    edges: Vec<[Vec3; 2]>,
}

impl MeshDetailLevel {
    fn new(mesh: MeshData) -> Self {
        Self {
            basic_mesh: Arc::new(mesh_to_basic(&mesh)),
            edges: primitives::extract_edges(&mesh),
            mesh,
        }
    }
}

/// Cached GPU-ready geometry for one persisted editable mesh.
struct EditableMeshEntry {
    revision: u64,
    mesh: Arc<MeshData>,
    basic: Arc<BasicMesh>,
    edges: Arc<Vec<[Vec3; 2]>>,
}

struct CurvedMeshDetail {
    levels: [MeshDetailLevel; 3],
}

impl CurvedMeshDetail {
    fn new(low: MeshData, medium: MeshData, high: MeshData) -> Self {
        Self {
            levels: [
                MeshDetailLevel::new(low),
                MeshDetailLevel::new(medium),
                MeshDetailLevel::new(high),
            ],
        }
    }

    fn level(&self, level: u8) -> &MeshDetailLevel {
        &self.levels[level.min(2) as usize]
    }

    fn medium(&self) -> &MeshDetailLevel {
        &self.levels[1]
    }
}

/// The CPU scene renderer.
///
/// Owns the framebuffer, mesh cache, and the budgeted texture ingress.
/// Retained state is limited to reusable allocations and budgeted caches.
pub struct SceneRenderer {
    /// The render target.
    framebuffer: Framebuffer,
    /// Cached mesh data per primitive and curved-mesh detail level.
    cube_mesh: MeshData,
    cube_basic_mesh: Arc<BasicMesh>,
    cube_edges: Vec<[Vec3; 2]>,
    cube_local_bounds: (Vec3, Vec3),
    capsule_detail: MeshDetailLevel,
    cylinder_detail: CurvedMeshDetail,
    cylinder_local_bounds: (Vec3, Vec3),
    sphere_detail: CurvedMeshDetail,
    sphere_local_bounds: (Vec3, Vec3),
    wedge_detail: MeshDetailLevel,
    wedge_local_bounds: (Vec3, Vec3),
    torus_detail: MeshDetailLevel,
    torus_local_bounds: (Vec3, Vec3),
    prism_detail: MeshDetailLevel,
    prism_local_bounds: (Vec3, Vec3),
    sphere2d_detail: MeshDetailLevel,
    sphere2d_local_bounds: (Vec3, Vec3),
    plane_mesh: MeshData,
    plane_basic_mesh: Arc<BasicMesh>,
    plane_edges: Vec<[Vec3; 2]>,
    plane_local_bounds: (Vec3, Vec3),
    world_transforms: Option<(u64, WorldTransformCache)>,
    render_jobs: Vec<RenderJob>,
    selected_ids: HashSet<SceneNodeId>,
    /// Reusable visibility history for the region-culling hysteresis band.
    stream_visible_ids: HashSet<SceneNodeId>,
    stream_history_revision: u64,
    /// Last selected curved-mesh LOD, retained to avoid detail popping.
    geometry_lod_by_id: HashMap<SceneNodeId, u8>,
    geometry_lod_history_revision: u64,
    asset_root: Option<PathBuf>,
    /// Budgeted decode/residency cache for project base-color textures.
    texture_ingress: TextureIngress,
    /// Authored relative texture path -> confined project path resolution.
    texture_paths: HashMap<String, Option<PathBuf>>,
    /// Monotonic frame counter used by ingress LRU and per-frame admission.
    texture_frame: u64,
    /// Output dimension cap applied to the last texture-quality policy.
    texture_max_dimension: u32,
    shadow_geometry: Option<Arc<crate::shadow_frame::ShadowGeometry>>,
    shadow_budget_exceeded: bool,
    shadow_geometry_revision: u64,
    model_mesh_cache: crate::model_mesh_cache::ModelMeshCache,
    skinning: skinning::SkinCache,
    model_basic_cache: HashMap<String, Arc<BasicMesh>>,
    /// Bounded cache of primitives with tiled UVs, keyed by primitive, LOD and
    /// the authored tiling bits. Texture sampling stays per-instance; only the
    /// UV attribute is regenerated.
    primitive_tiling_cache: HashMap<PrimitiveTilingKey, Arc<BasicMesh>>,
    /// Persisted Tab Mode topology compiled once per authored revision.
    editable_meshes: HashMap<(uuid::Uuid, u32, u32), EditableMeshEntry>,
    editable_prune_revision: u64,
    /// Stats from the last frame.
    pub stats: FrameStats,
    /// Retained per-entity particle simulators and render batches.
    particles: ParticleRenderer,
}

#[derive(Debug, Clone)]
pub struct SceneRenderFrame {
    pub commands: BasicCommandList,
    pub view_proj: Mat4,
    pub light_dir: Vec3,
    pub width: u32,
    pub height: u32,
    pub texture_cache_budget_bytes: u64,
    pub stats: FrameStats,
}

/// Fixed height of the world grid plane. The grid is intentionally
/// detached from both the camera and the scene content: it never follows
/// the camera height, never stretches to cover the scene bounds, and never
/// rides along an object being dragged. It stays flat and quiet.
pub const GRID_Y: f32 = -0.02;

/// Legacy neutral environment shared by the editor and local Play.
/// Scene-authored lighting overrides it only in explicit Scene mode.
pub const SCENE_BACKGROUND: [u8; 4] = [240, 240, 242, 255];
pub const SCENE_LIGHT_DIRECTION: Vec3 = Vec3::new(0.4, 0.8, 0.6);

impl SceneRenderer {
    /// Create a new renderer with initial viewport dimensions.
    pub fn new(width: u32, height: u32) -> Self {
        let cube_mesh = primitives::cube(1);
        let plane_mesh = primitives::plane(1);
        let cylinder_detail = CurvedMeshDetail::new(
            primitives::cylinder(12),
            primitives::cylinder(32),
            primitives::cylinder(48),
        );
        let sphere_detail = CurvedMeshDetail::new(
            primitives::sphere(8, 12),
            primitives::sphere(16, 24),
            primitives::sphere(24, 36),
        );
        let wedge_detail = MeshDetailLevel::new(primitives::wedge(1));
        let torus_detail = MeshDetailLevel::new(primitives::torus(32, 16));
        let prism_detail = MeshDetailLevel::new(primitives::prism(24));
        let sphere2d_detail = MeshDetailLevel::new(primitives::sphere2d(32, 16));
        let cube_local_bounds = cube_mesh.aabb();
        let cylinder_local_bounds = cylinder_detail.medium().mesh.aabb();
        let sphere_local_bounds = sphere_detail.medium().mesh.aabb();
        let wedge_local_bounds = wedge_detail.mesh.aabb();
        let torus_local_bounds = torus_detail.mesh.aabb();
        let prism_local_bounds = prism_detail.mesh.aabb();
        let sphere2d_local_bounds = sphere2d_detail.mesh.aabb();
        let plane_local_bounds = plane_mesh.aabb();

        Self {
            framebuffer: Framebuffer::new(width.max(1), height.max(1)),
            cube_basic_mesh: Arc::new(mesh_to_basic(&cube_mesh)),
            cube_edges: primitives::extract_edges(&cube_mesh),
            cube_local_bounds,
            cube_mesh,
            capsule_detail: MeshDetailLevel::new(primitives::capsule(16, 4)),
            cylinder_detail,
            cylinder_local_bounds,
            sphere_detail,
            sphere_local_bounds,
            wedge_detail,
            wedge_local_bounds,
            torus_detail,
            torus_local_bounds,
            prism_detail,
            prism_local_bounds,
            sphere2d_detail,
            sphere2d_local_bounds,
            plane_basic_mesh: Arc::new(mesh_to_basic(&plane_mesh)),
            plane_edges: primitives::extract_edges(&plane_mesh),
            plane_local_bounds,
            plane_mesh,
            world_transforms: None,
            render_jobs: Vec::new(),
            selected_ids: HashSet::new(),
            stream_visible_ids: HashSet::new(),
            stream_history_revision: 0,
            geometry_lod_by_id: HashMap::new(),
            geometry_lod_history_revision: 0,
            asset_root: None,
            texture_ingress: TextureIngress::new(
                crate::api_graphic_basic::asset_ingress::DEFAULT_TEXTURE_INGRESS_BUDGET_BYTES,
            ),
            texture_paths: HashMap::new(),
            texture_frame: 0,
            texture_max_dimension: TextureQualityMode::default().max_dimension(),
            shadow_geometry: None,
            shadow_budget_exceeded: false,
            shadow_geometry_revision: 0,
            model_mesh_cache: Default::default(),
            skinning: Default::default(),
            model_basic_cache: HashMap::new(),
            primitive_tiling_cache: HashMap::new(),
            editable_meshes: HashMap::new(),
            editable_prune_revision: 0,
            stats: FrameStats::default(),
            particles: ParticleRenderer::new(),
        }
    }

    /// Read-only particle state for authoring overlays and tests.
    pub fn particles(&self) -> &ParticleRenderer {
        &self.particles
    }

    /// Compile a persisted editable mesh to render geometry, reusing the
    /// compiled buffers until the authored revision or texture tiling changes.
    fn editable_geometry_in(
        cache: &mut HashMap<(uuid::Uuid, u32, u32), EditableMeshEntry>,
        node: &raf_core::scene::graph::SceneNode,
        tiling: [f32; 2],
    ) -> Option<(Arc<MeshData>, Arc<BasicMesh>, Arc<Vec<[Vec3; 2]>>)> {
        let data = node.editable_mesh.as_ref()?;
        if data.is_empty() {
            return None;
        }
        let revision = data.revision.max(1);
        let key = (node.uuid, tiling[0].to_bits(), tiling[1].to_bits());
        if let Some(entry) = cache.get(&key) {
            if entry.revision == revision {
                return Some((
                    Arc::clone(&entry.mesh),
                    Arc::clone(&entry.basic),
                    Arc::clone(&entry.edges),
                ));
            }
        }
        let editable = crate::editable::EditableMesh::from_scene_data(data);
        let mut mesh = editable.to_mesh_data_uv();
        scale_mesh_uvs(&mut mesh, tiling);
        let mesh = Arc::new(mesh);
        // Wire edges come from the welded authored topology, not the
        // per-face render vertices, so the selection outline stays single.
        let edges = Arc::new(editable.wireframe_edges());
        let basic = Arc::new(mesh_to_basic(mesh.as_ref()));
        if cache.len() >= MAX_EDITABLE_MESH_CACHE_ENTRIES {
            cache.clear();
        }
        cache.insert(
            key,
            EditableMeshEntry {
                revision,
                mesh: Arc::clone(&mesh),
                basic: Arc::clone(&basic),
                edges: Arc::clone(&edges),
            },
        );
        Some((mesh, basic, edges))
    }

    fn prune_editable_geometry(&mut self, scene: &SceneGraph) {
        if self.editable_prune_revision == scene.geometry_revision() {
            return;
        }
        self.editable_prune_revision = scene.geometry_revision();
        if self.editable_meshes.is_empty() {
            return;
        }
        let live = scene
            .iter()
            .filter(|(_, node)| node.editable_mesh.is_some())
            .map(|(_, node)| node.uuid)
            .collect::<HashSet<_>>();
        self.editable_meshes
            .retain(|(uuid, _, _), _| live.contains(uuid));
    }

    pub fn set_asset_root(&mut self, assets_root: Option<PathBuf>) {
        if self.asset_root != assets_root {
            self.asset_root = assets_root;
            self.texture_paths.clear();
            self.texture_ingress.clear();
            self.model_mesh_cache.clear();
            self.skinning = Default::default();
            self.model_basic_cache.clear();
        }
    }

    /// Resolve one authored texture path through the budgeted ingress.
    ///
    /// Relative paths are normalized once and cached; a resident texture hit
    /// allocates no new path or pixel buffer on later frames. The caller passes
    /// the three disjoint fields so mesh borrows stay alive during the scene
    /// loop without borrowing the whole renderer mutably.
    fn resolve_base_color_texture(
        asset_root: Option<&Path>,
        texture_paths: &mut HashMap<String, Option<PathBuf>>,
        texture_ingress: &mut TextureIngress,
        relative_path: &str,
        limits: TextureDecodeLimits,
        frame: u64,
    ) -> Result<Arc<CpuTexture>, String> {
        let root = asset_root.ok_or_else(|| "no project asset root is configured".to_string())?;
        if !texture_paths.contains_key(relative_path) {
            texture_paths.insert(
                relative_path.to_owned(),
                resolve_texture_candidate(root, relative_path),
            );
        }
        let path = texture_paths
            .get(relative_path)
            .and_then(|path| path.as_deref())
            .ok_or_else(|| "texture path is outside the project asset root".to_string())?;
        texture_ingress.get_or_decode(path, root, limits, frame)
    }

    /// Snapshot of texture ingress residency and cumulative counters.
    pub fn texture_ingress_metrics(
        &self,
    ) -> crate::api_graphic_basic::asset_ingress::TextureIngressMetrics {
        self.texture_ingress.metrics()
    }

    fn local_bounds_for_primitive(&self, primitive: Primitive) -> (Vec3, Vec3) {
        match primitive {
            Primitive::Cube => self.cube_local_bounds,
            Primitive::Capsule => (-Vec3::new(0.5, 1.0, 0.5), Vec3::new(0.5, 1.0, 0.5)),
            Primitive::Cylinder => self.cylinder_local_bounds,
            Primitive::Sphere => self.sphere_local_bounds,
            Primitive::Wedge => self.wedge_local_bounds,
            Primitive::Torus => self.torus_local_bounds,
            Primitive::Prism => self.prism_local_bounds,
            Primitive::Sphere2D => self.sphere2d_local_bounds,
            Primitive::Plane => self.plane_local_bounds,
            Primitive::Empty => self.cube_local_bounds,
        }
    }

    fn collect_render_jobs(
        &mut self,
        scene: &SceneGraph,
        frustum: &Frustum,
        camera_position: Vec3,
        camera_mode: CameraMode,
        view: Mat4,
        projection_scale_y: f32,
        viewport_height: f32,
        selected: &[SceneNodeId],
        options: RenderOptions,
        mesh_override: Option<(SceneNodeId, &MeshData)>,
    ) -> (Vec<RenderJob>, FrameStats) {
        self.selected_ids.clear();
        self.selected_ids.extend(selected.iter().copied());
        self.prune_editable_geometry(scene);
        let scene_revision = scene.geometry_revision();
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
        if !options.world_streaming_enabled {
            if self.stream_visible_ids.capacity() > 0 {
                self.stream_visible_ids = HashSet::new();
            }
            self.stream_history_revision = scene_revision;
        } else if self.stream_history_revision != scene_revision {
            self.stream_visible_ids
                .retain(|id| scene.is_valid_node(*id));
            self.stream_history_revision = scene_revision;
        }
        if options.geometry_detail != GeometryDetailMode::Adaptive {
            if self.geometry_lod_by_id.capacity() > 0 {
                self.geometry_lod_by_id = HashMap::new();
            }
            self.geometry_lod_history_revision = scene_revision;
        } else if self.geometry_lod_history_revision != scene_revision {
            self.geometry_lod_by_id
                .retain(|id, _| scene.is_valid_node(*id));
            self.geometry_lod_history_revision = scene_revision;
        }
        let mut jobs = std::mem::take(&mut self.render_jobs);
        jobs.clear();
        jobs.reserve(scene.len());
        let mut stats = FrameStats::default();
        let override_bounds = mesh_override.map(|(override_id, mesh)| (override_id, mesh.aabb()));
        let asset_root = self.asset_root.clone();

        for (id, node) in scene.iter() {
            if !node.visible
                || (matches!(node.primitive, Primitive::Empty)
                    && node.mesh_asset.is_none()
                    && node.editable_mesh.is_none())
            {
                self.stream_visible_ids.remove(&id);
                if options.geometry_detail == GeometryDetailMode::Adaptive {
                    self.geometry_lod_by_id.remove(&id);
                }
                continue;
            }
            stats.total_entities += 1;
            let skinned = node.skin.is_some();
            let tiling = sanitized_surface_tiling(node.surface.texture_tiling);
            let editable = Self::editable_geometry_in(&mut self.editable_meshes, node, tiling);
            let custom_mesh = editable
                .as_ref()
                .map(|(mesh, _, _)| Arc::clone(mesh))
                .or_else(|| {
                    node.mesh_asset.as_ref().and_then(|reference| {
                        if let Some(binding) = &node.skin {
                            self.skinning.mesh(
                                scene,
                                transforms,
                                id,
                                asset_root.as_deref()?,
                                reference,
                                binding,
                            )
                        } else {
                            self.model_mesh_cache
                                .get(asset_root.as_deref(), reference)
                                .ok()
                        }
                    })
                });
            if matches!(node.primitive, Primitive::Empty) && custom_mesh.is_none() {
                continue;
            }

            let model = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            // Sphere2D is a camera-facing disc: compose a billboard rotation
            // around the node's world pivot so it keeps its position.
            let model = if node.primitive == Primitive::Sphere2D {
                billboard_model(model, camera_position)
            } else {
                model
            };
            // The pivot only compensates the drawn geometry. The authored node
            // transform, gizmos, scripts and children stay on the node origin.
            let geometry_model = node_geometry_matrix(model, node);
            let (local_min, local_max) = override_bounds
                .and_then(|(override_id, bounds)| (override_id == id).then_some(bounds))
                .or_else(|| custom_mesh.as_ref().map(|mesh| mesh.aabb()))
                .unwrap_or_else(|| self.local_bounds_for_primitive(node.primitive));
            let bounds = SceneObjectBounds::from_local_aabb(local_min, local_max, geometry_model);

            let previously_in_stream =
                options.world_streaming_enabled && self.stream_visible_ids.contains(&id);
            let classification = if options.camera_navigation || options.stable_scene_packet {
                // A stable packet must not depend on which objects the moving
                // camera happens to keep inside the exact frustum. World
                // streaming still decides which regions are resident.
                if visibility.stream_allows(camera_position, bounds, previously_in_stream) {
                    SceneVisibility::Visible
                } else {
                    SceneVisibility::OutsideStream
                }
            } else {
                visibility.classify_with_hysteresis(
                    frustum,
                    camera_position,
                    bounds,
                    previously_in_stream,
                )
            };
            match classification {
                SceneVisibility::Visible => {
                    if options.world_streaming_enabled {
                        self.stream_visible_ids.insert(id);
                    }
                }
                SceneVisibility::OutsideFrustum => {
                    if options.world_streaming_enabled {
                        self.stream_visible_ids.insert(id);
                    }
                    stats.frustum_culled_entities += 1;
                    continue;
                }
                SceneVisibility::OutsideStream => {
                    self.stream_visible_ids.remove(&id);
                    stats.streaming_culled_entities += 1;
                    continue;
                }
            }

            stats.visible_entities += 1;
            let mut base_color = [node.color.r, node.color.g, node.color.b, node.color.a];
            if options.solid_xray_mode {
                base_color[3] = base_color[3].min(120);
            }
            if options.focus_node.is_some_and(|focus| focus != id.0 as u64) {
                for channel in &mut base_color[..3] {
                    *channel = ((*channel as f32) * 0.34) as u8;
                }
            }
            let center = bounds.center();
            let geometry_lod = if options.camera_navigation || options.stable_scene_packet {
                // Freeze LOD while navigating; recomputing it from the moving
                // projection would change mesh ids and invalidate the packet.
                self.geometry_lod_by_id.get(&id).copied().unwrap_or(1)
            } else if matches!(node.primitive, Primitive::Sphere | Primitive::Cylinder)
                && mesh_override.is_none_or(|(override_id, _)| override_id != id)
            {
                let lod = match options.geometry_detail {
                    GeometryDetailMode::Efficient => 0,
                    GeometryDetailMode::Adaptive => {
                        let projected_radius = projected_radius_pixels(
                            bounds,
                            camera_mode,
                            view,
                            projection_scale_y,
                            viewport_height,
                        );
                        adaptive_geometry_lod(
                            projected_radius,
                            self.geometry_lod_by_id.get(&id).copied(),
                        )
                    }
                    GeometryDetailMode::Detailed => 2,
                };
                if options.geometry_detail == GeometryDetailMode::Adaptive
                    && (self.geometry_lod_by_id.contains_key(&id)
                        || self.geometry_lod_by_id.len() < MAX_GEOMETRY_LOD_HISTORY_ENTRIES)
                {
                    self.geometry_lod_by_id.insert(id, lod);
                }
                lod
            } else if options.geometry_detail == GeometryDetailMode::Adaptive {
                self.geometry_lod_by_id.remove(&id);
                1
            } else {
                1
            };

            let custom_edges = editable.as_ref().map(|(_, _, edges)| Arc::clone(edges));
            let custom_basic = editable
                .as_ref()
                .map(|(_, basic, _)| Arc::clone(basic))
                .or_else(|| {
                    custom_mesh.as_ref().map(|mesh| {
                        if skinned {
                            return Arc::new(mesh_to_basic_with_tiling(mesh, tiling));
                        }
                        let key = node
                            .mesh_asset
                            .as_ref()
                            .map(|reference| {
                                format!(
                                    "{}|{}|{}|{}|{}|{}",
                                    reference.path,
                                    reference.content_hash.as_deref().unwrap_or(""),
                                    reference.node.map(|node| node as i64).unwrap_or(-1),
                                    reference.import_scale.to_bits(),
                                    tiling[0].to_bits(),
                                    tiling[1].to_bits()
                                )
                            })
                            .unwrap_or_else(|| {
                                format!("{}|{}", tiling[0].to_bits(), tiling[1].to_bits())
                            });
                        if let Some(basic) = self.model_basic_cache.get(&key) {
                            Arc::clone(basic)
                        } else {
                            let basic = Arc::new(mesh_to_basic_with_tiling(mesh, tiling));
                            if self.model_basic_cache.len() >= 64 {
                                self.model_basic_cache.clear();
                            }
                            self.model_basic_cache.insert(key, Arc::clone(&basic));
                            basic
                        }
                    })
                });
            jobs.push(RenderJob {
                id,
                primitive: node.primitive,
                geometry_lod,
                model: geometry_model,
                base_color,
                is_selected: self.selected_ids.contains(&id),
                distance_squared: (center - camera_position).length_squared(),
                is_transparent: base_color[3] < u8::MAX,
                base_color_texture: node.base_color_texture.clone(),
                unlit: node.surface.unlit,
                // Authored topology has no canonical "outside": a face moved
                // through its own plane must stay visible instead of turning
                // black or disappearing. Tab Mode and imported static meshes
                // are always two-sided; the shader uses abs(N dot L) for them.
                double_sided: node.surface.double_sided
                    || node.editable_mesh.is_some()
                    || node.mesh_asset.is_some(),
                tiling,
                custom_basic,
                custom_edges,
                skinned,
                custom_key: custom_mesh
                    .as_ref()
                    .map(|mesh| Arc::as_ptr(mesh) as *const () as usize as u64)
                    .unwrap_or(0),
                custom_mesh,
            });
        }

        // Opaque geometry is grouped by identical primitive/mesh/material so
        // the command list can batch draws. The order is deliberately
        // camera-independent: camera movement must not invalidate a retained
        // scene packet, because AGB packet replacement re-uploads geometry and
        // textures. Transparent geometry is stable by entity id for the same
        // reason; per-sprite camera sorting belongs in a dedicated pass.
        jobs.sort_by(|a, b| {
            a.is_transparent
                .cmp(&b.is_transparent)
                .then_with(|| {
                    primitive_batch_key(a.primitive).cmp(&primitive_batch_key(b.primitive))
                })
                .then_with(|| a.custom_key.cmp(&b.custom_key))
                .then_with(|| a.geometry_lod.cmp(&b.geometry_lod))
                .then_with(|| {
                    a.tiling[0]
                        .to_bits()
                        .cmp(&b.tiling[0].to_bits())
                        .then_with(|| a.tiling[1].to_bits().cmp(&b.tiling[1].to_bits()))
                })
                .then_with(|| a.id.0.cmp(&b.id.0))
        });

        (jobs, stats)
    }

    fn build_shadows(
        &mut self,
        scene: &SceneGraph,
        camera: Vec3,
        environment: Option<crate::environment_frame::EnvironmentFrame>,
        mesh_override: Option<(SceneNodeId, &MeshData)>,
    ) -> Option<crate::shadow_frame::ShadowFrame> {
        use crate::shadow_frame::*;
        self.shadow_budget_exceeded = false;
        let Some(env) = environment.filter(|e| {
            e.settings.shadows.enabled
                && e.settings.shadows.strength > 0.0
                && e.sun.max_element() > 0.0
        }) else {
            self.shadow_geometry = None;
            return None;
        };
        let settings = env.settings.shadows;
        let resolution = settings.resolution as u32;
        let vp = if env.improved() {
            fitted_directional_projection(
                env.sun_direction,
                camera,
                env.inverse_view_projection,
                settings.distance,
                resolution,
            )
        } else {
            directional_projection(env.sun_direction, camera, settings.distance, resolution)
        };
        if mesh_override.is_none() && self.shadow_geometry_revision == scene.geometry_revision() {
            if let Some(geometry) = self
                .shadow_geometry
                .as_ref()
                .filter(|g| g.view_projection == vp && g.resolution == resolution)
            {
                return Some(ShadowFrame {
                    geometry: Arc::clone(geometry),
                    settings,
                    camera,
                    improved: env.improved(),
                });
            }
        }
        let transforms = &self
            .world_transforms
            .as_ref()
            .expect("collected scene transforms")
            .1;
        let mut casters = Vec::new();
        let mut triangles: usize = 0;
        let mut unique_meshes = HashSet::new();
        let mut mesh_bytes = 0;
        for (id, node) in scene.iter() {
            if !node.visible
                || !node.casts_shadows
                || node.color.a < 255
                || (node.primitive == Primitive::Empty
                    && node.mesh_asset.is_none()
                    && node.editable_mesh.is_none())
            {
                continue;
            }
            let model = transforms
                .world_matrix(id)
                .unwrap_or_else(|| node.local_matrix());
            let model = node_geometry_matrix(model, node);
            if !model.is_finite() {
                continue;
            }
            let editable = Self::editable_geometry_in(
                &mut self.editable_meshes,
                node,
                sanitized_surface_tiling(node.surface.texture_tiling),
            );
            let bounds = mesh_override
                .filter(|(i, _)| *i == id)
                .map(|(_, m)| m.aabb())
                .or_else(|| editable.as_ref().map(|(mesh, _, _)| mesh.aabb()))
                .unwrap_or_else(|| self.local_bounds_for_primitive(node.primitive));
            let mut min = Vec3::splat(f32::INFINITY);
            let mut max = Vec3::splat(f32::NEG_INFINITY);
            for x in [bounds.0.x, bounds.1.x] {
                for y in [bounds.0.y, bounds.1.y] {
                    for z in [bounds.0.z, bounds.1.z] {
                        let p = (vp * model).transform_point3(Vec3::new(x, y, z));
                        min = min.min(p);
                        max = max.max(p);
                    }
                }
            }
            // Deliberately not camera-frustum culled: offscreen geometry casts visible shadows.
            if min.x > 1.0
                || max.x < -1.0
                || min.y > 1.0
                || max.y < -1.0
                || min.z > 1.0
                || max.z < 0.0
            {
                continue;
            }
            let mesh = if let Some((_, m)) = mesh_override.filter(|(i, _)| *i == id) {
                // Admission precedes copying editable geometry into a second frame mesh.
                let bytes = m
                    .positions
                    .len()
                    .saturating_mul(32)
                    .saturating_add(m.indices.len().saturating_mul(4));
                if bytes.saturating_add(mesh_bytes) > MAX_SHADOW_MESH_BYTES
                    || triangles.saturating_add(m.indices.len() / 3) > MAX_SHADOW_TRIANGLES
                {
                    self.shadow_geometry = None;
                    self.shadow_budget_exceeded = true;
                    return None;
                }
                Arc::new(mesh_to_basic(m))
            } else if let Some((_, basic, _)) = editable {
                basic
            } else {
                match node.primitive {
                    Primitive::Plane => Arc::clone(&self.plane_basic_mesh),
                    Primitive::Capsule => Arc::clone(&self.capsule_detail.basic_mesh),
                    Primitive::Cylinder => Arc::clone(&self.cylinder_detail.level(1).basic_mesh),
                    Primitive::Sphere => Arc::clone(&self.sphere_detail.level(1).basic_mesh),
                    Primitive::Wedge => Arc::clone(&self.wedge_detail.basic_mesh),
                    Primitive::Torus => Arc::clone(&self.torus_detail.basic_mesh),
                    Primitive::Prism => Arc::clone(&self.prism_detail.basic_mesh),
                    Primitive::Sphere2D => Arc::clone(&self.sphere2d_detail.basic_mesh),
                    _ => Arc::clone(&self.cube_basic_mesh),
                }
            };
            triangles += mesh.indices.len() / 3;
            if unique_meshes.insert(Arc::as_ptr(&mesh) as usize) {
                mesh_bytes += mesh.vertices.len() * 32 + mesh.indices.len() * 4;
            }
            if casters.len() >= MAX_SHADOW_CASTERS
                || triangles > MAX_SHADOW_TRIANGLES
                || mesh_bytes > MAX_SHADOW_MESH_BYTES
            {
                // Never silently render a partial set, which would make shadows disappear arbitrarily.
                self.shadow_geometry = None;
                self.shadow_budget_exceeded = true;
                return None;
            }
            casters.push(ShadowCaster { mesh, model });
        }
        if casters.is_empty() {
            self.shadow_geometry = None;
            return None;
        }
        let geometry = ShadowGeometry::new(vp, resolution, casters);
        if self
            .shadow_geometry
            .as_ref()
            .is_none_or(|old| old.key != geometry.key)
        {
            self.shadow_geometry = Some(Arc::new(geometry));
        }
        self.shadow_geometry_revision = if mesh_override.is_none() {
            scene.geometry_revision()
        } else {
            0
        };
        Some(ShadowFrame {
            geometry: Arc::clone(self.shadow_geometry.as_ref().unwrap()),
            settings,
            camera,
            improved: env.improved(),
        })
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
        let frame = self.build_frame(
            scene,
            camera,
            vp_w,
            vp_h,
            selected,
            bg_color,
            light_dir,
            options,
            mesh_override,
        );
        self.stats = frame.stats.clone();
        rasterize_basic_scene_frame(&frame, &mut self.framebuffer);
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
        let environment = crate::environment_frame::EnvironmentFrame::new(
            options.environment_override.unwrap_or(*scene.environment()),
            vp,
        );
        let light_dir = environment
            .map(|env| env.sun_direction)
            .unwrap_or_else(|| light_dir.normalize());
        let cam_eye = camera.eye();
        let particle_options = options.particles.clone();
        let particle_dt = options.particle_dt.clamp(0.0, 0.1);
        let particle_asset_root = self.asset_root.clone();
        let (jobs, mut stats) = self.collect_render_jobs(
            scene,
            &frustum,
            cam_eye,
            camera.mode,
            view,
            proj.y_axis.y,
            h as f32,
            selected,
            options,
            mesh_override,
        );
        let shadows = self.build_shadows(scene, cam_eye, environment, mesh_override);

        // The residency cache uses the full project texture budget. The
        // per-frame admission cap only limits newly decoded bytes; resident
        // hits are free. Splitting the cache in half rejected textures that
        // were already decoded and caused decode/evict churn in textured
        // scenes.
        self.texture_frame = self.texture_frame.wrapping_add(1).max(1);
        let texture_frame = self.texture_frame;
        let texture_budget_bytes = options.texture_quality.cache_budget_bytes();
        let texture_resident_budget = texture_budget_bytes.saturating_mul(3) / 4;
        let texture_frame_budget = texture_budget_bytes.saturating_sub(texture_resident_budget);
        let texture_max_dimension = options.texture_quality.max_dimension();
        if texture_max_dimension != self.texture_max_dimension {
            self.texture_max_dimension = texture_max_dimension;
            self.texture_ingress.clear();
        }
        self.texture_ingress.set_budget(texture_resident_budget);
        self.texture_ingress.set_frame_budget(texture_frame_budget);
        let texture_limits = TextureDecodeLimits {
            max_dimension: texture_max_dimension,
            ..TextureDecodeLimits::default()
        };

        let cube_mesh = &self.cube_mesh;
        let cube_basic_mesh = Arc::clone(&self.cube_basic_mesh);
        let cube_edges = self.cube_edges.as_slice();
        let capsule_detail = &self.capsule_detail;
        let cylinder_detail = &self.cylinder_detail;
        let sphere_detail = &self.sphere_detail;
        let wedge_detail = &self.wedge_detail;
        let torus_detail = &self.torus_detail;
        let prism_detail = &self.prism_detail;
        let sphere2d_detail = &self.sphere2d_detail;
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
        commands.camera_eye = cam_eye;
        commands.environment = environment;
        commands.optimized_improved = options
            .environment_override
            .unwrap_or(*scene.environment())
            .technology
            == raf_core::environment::LightingTechnology::OptimizedImproved;
        commands.shadows = shadows;
        commands.shadow_budget_exceeded = self.shadow_budget_exceeded;
        commands.entity_lights = crate::entity_lights_frame::collect_entity_lights(scene, cam_eye);

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

            let (mesh, basic_mesh): (&MeshData, Arc<BasicMesh>) = if let Some(job_override_mesh) =
                job_override_mesh
            {
                let basic = if job.tiling == [1.0, 1.0] {
                    override_basic_mesh
                        .as_ref()
                        .map(Arc::clone)
                        .unwrap_or_else(|| Arc::new(mesh_to_basic(job_override_mesh)))
                } else {
                    Arc::new(mesh_to_basic_with_tiling(job_override_mesh, job.tiling))
                };
                (job_override_mesh, basic)
            } else if let (Some(custom), Some(basic)) = (&job.custom_mesh, &job.custom_basic) {
                (custom.as_ref(), Arc::clone(basic))
            } else {
                let (mesh, base_basic) = match job.primitive {
                    Primitive::Capsule => {
                        (&capsule_detail.mesh, Arc::clone(&capsule_detail.basic_mesh))
                    }
                    Primitive::Cube => (cube_mesh, Arc::clone(&cube_basic_mesh)),
                    Primitive::Cylinder => {
                        let detail = cylinder_detail.level(job.geometry_lod);
                        (&detail.mesh, Arc::clone(&detail.basic_mesh))
                    }
                    Primitive::Sphere => {
                        let detail = sphere_detail.level(job.geometry_lod);
                        (&detail.mesh, Arc::clone(&detail.basic_mesh))
                    }
                    Primitive::Wedge => (&wedge_detail.mesh, Arc::clone(&wedge_detail.basic_mesh)),
                    Primitive::Torus => (&torus_detail.mesh, Arc::clone(&torus_detail.basic_mesh)),
                    Primitive::Prism => (&prism_detail.mesh, Arc::clone(&prism_detail.basic_mesh)),
                    Primitive::Sphere2D => (
                        &sphere2d_detail.mesh,
                        Arc::clone(&sphere2d_detail.basic_mesh),
                    ),
                    Primitive::Plane => (plane_mesh, Arc::clone(&plane_basic_mesh)),
                    _ => (cube_mesh, Arc::clone(&cube_basic_mesh)),
                };
                let basic = tiled_primitive_basic(
                    &mut self.primitive_tiling_cache,
                    PrimitiveTilingKey {
                        primitive: primitive_batch_key(job.primitive),
                        lod: job.geometry_lod,
                        tiling: [job.tiling[0].to_bits(), job.tiling[1].to_bits()],
                    },
                    &base_basic,
                    job.tiling,
                );
                (mesh, basic)
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

            let texture_id = match job.base_color_texture.as_deref() {
                Some(path) => Self::resolve_base_color_texture(
                    self.asset_root.as_deref(),
                    &mut self.texture_paths,
                    &mut self.texture_ingress,
                    path,
                    texture_limits,
                    texture_frame,
                )
                .ok()
                .map(|texture| commands.register_texture(texture)),
                None => None,
            };

            let lit = use_tonality && !job.unlit;
            let two_sided =
                job.double_sided || matches!(job.primitive, Primitive::Plane | Primitive::Sphere2D);
            commands.set_pipeline(match (lit, two_sided) {
                (true, true) => BasicPipelineKind::PbrLitTwoSided,
                (true, false) => BasicPipelineKind::PbrLit,
                (false, true) => BasicPipelineKind::FlatColorTwoSided,
                (false, false) => BasicPipelineKind::FlatColor,
            });
            let mesh_id = if job_override_mesh.is_some() || job.skinned {
                commands.register_transient_mesh(basic_mesh)
            } else {
                commands.register_mesh(basic_mesh)
            };
            if commands.shadows.is_some() {
                commands.set_shadow_receiver(scene.get(job.id).is_some_and(|n| n.receives_shadows));
            }
            commands.draw_mesh_with_texture(mesh_id, job.model, color, texture_id);
            stats.triangles_rendered += mesh.triangle_count() as u32;
        }

        // Edges are intentionally recorded after all mesh commands. Interleaving
        // one line command per object would split otherwise compatible mesh
        // runs and defeat instancing when surface edges are enabled.
        for job in &jobs {
            let draw_surface_edges = options.solid_show_surface_edges;
            if draw_surface_edges || (options.selection_outline && job.is_selected) {
                let edges = if let Some(custom_edges) = &job.custom_edges {
                    custom_edges.as_slice()
                } else if mesh_override.is_some_and(|(override_id, _)| override_id == job.id) {
                    override_edges.as_deref().unwrap_or(&[])
                } else {
                    match job.primitive {
                        Primitive::Capsule => capsule_detail.edges.as_slice(),
                        Primitive::Cube => cube_edges,
                        Primitive::Cylinder => {
                            cylinder_detail.level(job.geometry_lod).edges.as_slice()
                        }
                        Primitive::Sphere => sphere_detail.level(job.geometry_lod).edges.as_slice(),
                        Primitive::Wedge => wedge_detail.edges.as_slice(),
                        Primitive::Torus => torus_detail.edges.as_slice(),
                        Primitive::Prism => prism_detail.edges.as_slice(),
                        Primitive::Sphere2D => sphere2d_detail.edges.as_slice(),
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

        self.particles.update_and_draw(
            scene,
            camera,
            particle_asset_root.as_deref(),
            &particle_options,
            &mut commands,
            particle_dt,
        );

        SceneRenderFrame {
            commands,
            view_proj: vp,
            light_dir,
            width: w,
            height: h,
            texture_cache_budget_bytes: options.texture_quality.cache_budget_bytes(),
            stats,
        }
    }

    /// Get the framebuffer dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.framebuffer.width(), self.framebuffer.height())
    }
}

/// Normalize an authored texture path and confine it to the project asset root.
///
/// Traversal components, absolute paths, and drive prefixes are rejected before
/// the ingress ever touches the filesystem.
fn resolve_texture_candidate(root: &Path, relative_path: &str) -> Option<PathBuf> {
    let normalized = relative_path.replace('\\', "/");
    let mut relative = PathBuf::new();
    for component in Path::new(&normalized).components() {
        match component {
            Component::Normal(value) => relative.push(value),
            _ => return None,
        }
    }
    if relative.as_os_str().is_empty() {
        return None;
    }
    let resolved = root.join(relative);
    resolved.starts_with(root).then_some(resolved)
}

pub(crate) fn rasterize_basic_scene_frame(frame: &SceneRenderFrame, framebuffer: &mut Framebuffer) {
    framebuffer.resize(frame.width, frame.height);
    let shadows = frame.commands.shadows.as_ref().filter(|s| {
        s.geometry.cpu_depth();
        !s.geometry.cpu_budget_exceeded()
    });
    let vp_w = frame.width as f32;
    let vp_h = frame.height as f32;
    let mut current_pipeline = BasicPipelineKind::FlatColor;
    let mut receives_shadows = true;

    // Keep the frame command list borrowed; cloning a line batch here would
    // allocate on every CPU fallback frame.
    for command in frame.commands.commands() {
        match command {
            GraphicCommand::Clear { r, g, b, a } => {
                framebuffer.clear(*r, *g, *b, *a);
                if let Some(env) = frame.commands.environment {
                    env.rasterize_sky(framebuffer);
                }
            }
            GraphicCommand::SetPipeline(pipeline) => current_pipeline = *pipeline,
            GraphicCommand::SetShadowReceiver(value) => receives_shadows = *value,
            GraphicCommand::DrawMesh {
                mesh_id,
                transform,
                color,
                texture_id,
            } => {
                let Some(mesh) = frame.commands.mesh(*mesh_id) else {
                    continue;
                };
                let texture = texture_id
                    .and_then(|id| frame.commands.texture(id))
                    .map(Arc::as_ref);
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
                    texture,
                    frame.commands.environment,
                    shadows.filter(|_| receives_shadows),
                    &frame.commands.entity_lights,
                );
            }
            GraphicCommand::DrawMeshBatch {
                mesh_id,
                texture_id,
                instances,
            } => {
                let Some(mesh) = frame.commands.mesh(*mesh_id) else {
                    continue;
                };
                let texture = texture_id
                    .and_then(|id| frame.commands.texture(id))
                    .map(Arc::as_ref);
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
                        texture,
                        frame.commands.environment,
                        shadows.filter(|_| receives_shadows),
                        &frame.commands.entity_lights,
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
    geometry_lod: u8,
    model: Mat4,
    base_color: [u8; 4],
    is_selected: bool,
    distance_squared: f32,
    is_transparent: bool,
    base_color_texture: Option<String>,
    unlit: bool,
    double_sided: bool,
    tiling: [f32; 2],
    custom_mesh: Option<Arc<MeshData>>,
    custom_basic: Option<Arc<BasicMesh>>,
    custom_edges: Option<Arc<Vec<[Vec3; 2]>>>,
    skinned: bool,
    custom_key: u64,
}

/// Identity of a primitive mesh variant with tiled UVs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PrimitiveTilingKey {
    primitive: u8,
    lod: u8,
    tiling: [u32; 2],
}

const MAX_PRIMITIVE_TILING_CACHE_ENTRIES: usize = 64;
/// Hard cap for editable topology variants. A node can legitimately change
/// tiling while editing; clearing the whole cache on overflow keeps memory
/// bounded without per-frame eviction bookkeeping.
const MAX_EDITABLE_MESH_CACHE_ENTRIES: usize = 256;

const LOW_TO_MEDIUM_LOD_ENTER_RADIUS_PX: f32 = 18.0;
const MEDIUM_TO_LOW_LOD_EXIT_RADIUS_PX: f32 = 12.0;
const MEDIUM_TO_HIGH_LOD_ENTER_RADIUS_PX: f32 = 72.0;
const HIGH_TO_MEDIUM_LOD_EXIT_RADIUS_PX: f32 = 56.0;
const MAX_GEOMETRY_LOD_HISTORY_ENTRIES: usize = 16_384;

/// Rotate `model` so its local +Z axis points at the camera, using world up as
/// the vertical reference. Translation and scale are taken from the node model;
/// the authored rotation is overridden, as a billboard must always face the
/// camera, and the node keeps its world position.
fn billboard_model(model: Mat4, camera_position: Vec3) -> Mat4 {
    let pivot = model.w_axis.truncate();
    let forward = (camera_position - pivot).normalize_or_zero();
    if forward.length_squared() < 0.5 || !forward.is_finite() {
        return model;
    }
    let right = Vec3::Y.cross(forward);
    let right = if right.length_squared() < 1e-8 {
        // Camera is directly above or below: pick a stable tangent.
        Vec3::X
    } else {
        right.normalize()
    };
    let up = forward.cross(right).normalize_or_zero();
    if up.length_squared() < 0.5 {
        return model;
    }
    let scale = Vec3::new(
        model.x_axis.length(),
        model.y_axis.length(),
        model.z_axis.length(),
    );
    Mat4::from_cols(
        (right * scale.x).extend(0.0),
        (up * scale.y).extend(0.0),
        (forward * scale.z).extend(0.0),
        pivot.extend(1.0),
    )
}

fn projected_radius_pixels(
    bounds: SceneObjectBounds,
    camera_mode: CameraMode,
    view: Mat4,
    projection_scale_y: f32,
    viewport_height: f32,
) -> f32 {
    let view_center = view.transform_point3(bounds.center());
    let radius = (bounds.max - bounds.min).length() * 0.5;
    let view_depth = match camera_mode {
        CameraMode::Perspective => (-view_center.z).max(0.1),
        CameraMode::Orthographic => 1.0,
    };
    let projected = radius * projection_scale_y.abs() * viewport_height.max(1.0) * 0.5 / view_depth;
    if projected.is_finite() {
        projected.max(0.0)
    } else {
        0.0
    }
}

fn adaptive_geometry_lod(projected_radius_px: f32, previous_lod: Option<u8>) -> u8 {
    let radius = if projected_radius_px.is_finite() {
        projected_radius_px.max(0.0)
    } else {
        0.0
    };
    match previous_lod {
        Some(0) if radius < LOW_TO_MEDIUM_LOD_ENTER_RADIUS_PX => 0,
        Some(0) if radius >= MEDIUM_TO_HIGH_LOD_ENTER_RADIUS_PX => 2,
        Some(0) => 1,
        Some(1) if radius <= MEDIUM_TO_LOW_LOD_EXIT_RADIUS_PX => 0,
        Some(1) if radius >= MEDIUM_TO_HIGH_LOD_ENTER_RADIUS_PX => 2,
        Some(1) => 1,
        Some(2) if radius > HIGH_TO_MEDIUM_LOD_EXIT_RADIUS_PX => 2,
        Some(2) if radius <= MEDIUM_TO_LOW_LOD_EXIT_RADIUS_PX => 0,
        Some(2) => 1,
        _ if radius < LOW_TO_MEDIUM_LOD_ENTER_RADIUS_PX => 0,
        _ if radius < MEDIUM_TO_HIGH_LOD_ENTER_RADIUS_PX => 1,
        _ => 2,
    }
}

#[inline]
const fn primitive_batch_key(primitive: Primitive) -> u8 {
    match primitive {
        Primitive::Cube => 0,
        Primitive::Capsule => 5,
        Primitive::Cylinder => 1,
        Primitive::Sphere => 2,
        Primitive::Plane => 3,
        Primitive::Empty => 4,
        Primitive::Wedge => 6,
        Primitive::Torus => 7,
        Primitive::Prism => 8,
        Primitive::Sphere2D => 9,
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
                    uv: mesh.uvs.get(index).copied().unwrap_or([0.0, 0.0]),
                },
            )
            .collect(),
        mesh.indices.clone(),
    )
}

fn mesh_to_basic_with_tiling(mesh: &MeshData, tiling: [f32; 2]) -> BasicMesh {
    let mut basic = mesh_to_basic(mesh);
    if tiling != [1.0, 1.0] {
        for vertex in &mut basic.vertices {
            vertex.uv[0] *= tiling[0];
            vertex.uv[1] *= tiling[1];
        }
    }
    basic
}

fn scale_mesh_uvs(mesh: &mut MeshData, tiling: [f32; 2]) {
    if tiling == [1.0, 1.0] {
        return;
    }
    for uv in &mut mesh.uvs {
        uv[0] *= tiling[0];
        uv[1] *= tiling[1];
    }
}

/// The authored surface may come from a hand-edited document. Keep the
/// renderer total: invalid tiling falls back to the identity repeat.
fn sanitized_surface_tiling(tiling: [f32; 2]) -> [f32; 2] {
    let axis = |value: f32| {
        if value.is_finite() && value > 0.0 {
            value.clamp(0.001, 128.0)
        } else {
            1.0
        }
    };
    [axis(tiling[0]), axis(tiling[1])]
}

fn tiled_primitive_basic(
    cache: &mut HashMap<PrimitiveTilingKey, Arc<BasicMesh>>,
    key: PrimitiveTilingKey,
    base: &Arc<BasicMesh>,
    tiling: [f32; 2],
) -> Arc<BasicMesh> {
    if tiling == [1.0, 1.0] {
        return Arc::clone(base);
    }
    if let Some(mesh) = cache.get(&key) {
        return Arc::clone(mesh);
    }
    let mut vertices = base.vertices.clone();
    for vertex in &mut vertices {
        vertex.uv[0] *= tiling[0];
        vertex.uv[1] *= tiling[1];
    }
    let mesh = Arc::new(BasicMesh::new(vertices, base.indices.clone()));
    if cache.len() >= MAX_PRIMITIVE_TILING_CACHE_ENTRIES {
        cache.clear();
    }
    cache.insert(key, Arc::clone(&mesh));
    mesh
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
    texture: Option<&crate::texture::CpuTexture>,
    two_sided: bool,
    vp_w: f32,
    vp_h: f32,
    environment: Option<crate::environment_frame::EnvironmentFrame>,
    shadows: Option<&crate::shadow_frame::ShadowFrame>,
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
            z: ndc_z,
            shade: vertex.shade,
        })
    };

    let Some(first) = to_screen(vertices[0]) else {
        return 0;
    };
    let to_textured_screen = |vertex: ClipVertex| -> Option<rasterizer::TexturedScreenVertex> {
        if vertex.position.w <= 1.0e-6 {
            return None;
        }
        let inv_w = 1.0 / vertex.position.w;
        let ndc_x = vertex.position.x * inv_w;
        let ndc_y = vertex.position.y * inv_w;
        let ndc_z = vertex.position.z * inv_w;
        Some(rasterizer::TexturedScreenVertex {
            x: (ndc_x + 1.0) * 0.5 * vp_w,
            y: (1.0 - ndc_y) * 0.5 * vp_h,
            z: ndc_z,
            shade: vertex.shade,
            uv_over_w: [vertex.uv[0] * inv_w, vertex.uv[1] * inv_w],
            inv_w,
        })
    };
    let mut rendered = 0u32;
    for index in 1..vertices.len() - 1 {
        let (Some(mut second), Some(mut third)) =
            (to_screen(vertices[index]), to_screen(vertices[index + 1]))
        else {
            continue;
        };
        if texture.is_some() || environment.is_some() {
            let (Some(textured_first), Some(mut textured_second), Some(mut textured_third)) = (
                to_textured_screen(vertices[0]),
                to_textured_screen(vertices[index]),
                to_textured_screen(vertices[index + 1]),
            ) else {
                continue;
            };
            let textured_area = (textured_second.x - textured_first.x)
                * (textured_third.y - textured_first.y)
                - (textured_third.x - textured_first.x) * (textured_second.y - textured_first.y);
            if two_sided && textured_area >= 0.0 {
                std::mem::swap(&mut textured_second, &mut textured_third);
            }
            rasterizer::rasterize_triangle_lighting_shadowed(
                framebuffer,
                [textured_first, textured_second, textured_third],
                texture,
                color,
                environment.map(|env| crate::optimized_lighting::SurfaceLighting {
                    ambient: env.ambient,
                    sun: env.sun,
                    steps: env.settings.sun.shading_steps(),
                    softness: env.softness(),
                    exposure: env.exposure(),
                }),
                shadows
                    .zip(environment)
                    .map(|(shadow, env)| (shadow, env.inverse_view_projection)),
            );
        } else if color[3] < u8::MAX {
            let area = (second.x - first.x) * (third.y - first.y)
                - (third.x - first.x) * (second.y - first.y);
            if two_sided && area >= 0.0 {
                std::mem::swap(&mut second, &mut third);
            }
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
            let area = (second.x - first.x) * (third.y - first.y)
                - (third.x - first.x) * (second.y - first.y);
            if two_sided && area >= 0.0 {
                std::mem::swap(&mut second, &mut third);
            }
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
    texture: Option<&crate::texture::CpuTexture>,
    environment: Option<crate::environment_frame::EnvironmentFrame>,
    shadows: Option<&crate::shadow_frame::ShadowFrame>,
    entity_lights: &crate::entity_lights_frame::EntityLightFrame,
) {
    let mvp = *view_proj * *model;
    let normal_mat = transform::normal_matrix(model);
    let shaded = pipeline.is_lit();
    let two_sided = matches!(
        pipeline,
        BasicPipelineKind::FlatColorTwoSided
            | BasicPipelineKind::PbrLitTwoSided
            | BasicPipelineKind::Particle
    );
    let diffuse = |normal: Vec3| {
        let value = normal.dot(light_dir);
        if two_sided {
            value.abs()
        } else {
            value.max(0.0)
        }
    };
    let authored_ui = matches!(
        pipeline,
        BasicPipelineKind::UiSurface | BasicPipelineKind::UiSurfaceOverlay
    );
    if authored_ui {
        fb.set_depth_policy(pipeline != BasicPipelineKind::UiSurfaceOverlay, false);
    } else if pipeline == BasicPipelineKind::Particle {
        // Particle billboards read scene depth but never write it.
        fb.set_depth_policy(true, false);
    }

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
            let normal = transform::transform_normal(mesh.vertices[i0].normal, &normal_mat);
            let world = model.transform_point3(p0);
            0.3 + 0.7 * diffuse(normal) + entity_lights.shade(normal, world)
        } else {
            1.0
        };
        let shade1 = if shaded {
            let normal = transform::transform_normal(mesh.vertices[i1].normal, &normal_mat);
            let world = model.transform_point3(p1);
            0.3 + 0.7 * diffuse(normal) + entity_lights.shade(normal, world)
        } else {
            1.0
        };
        let shade2 = if shaded {
            let normal = transform::transform_normal(mesh.vertices[i2].normal, &normal_mat);
            let world = model.transform_point3(p2);
            0.3 + 0.7 * diffuse(normal) + entity_lights.shade(normal, world)
        } else {
            1.0
        };

        let (shade0, shade1, shade2) = if environment.is_some() && shaded {
            // Reuse the Gouraud interpolator for N dot L; RGB light is applied in linear space.
            let shade = |i: usize, position: Vec3| {
                let normal = transform::transform_normal(mesh.vertices[i].normal, &normal_mat);
                let world = model.transform_point3(position);
                diffuse(normal) + entity_lights.shade(normal, world)
            };
            (shade(i0, p0), shade(i1, p1), shade(i2, p2))
        } else {
            (shade0, shade1, shade2)
        };
        let (shade0, shade1, shade2) = if two_sided {
            (shade0.max(0.35), shade1.max(0.35), shade2.max(0.35))
        } else {
            (shade0, shade1, shade2)
        };
        rasterize_clipped_triangle(
            fb,
            [
                ClipVertex {
                    position: c0,
                    shade: shade0,
                    uv: mesh.vertices[i0].uv,
                },
                ClipVertex {
                    position: c1,
                    shade: shade1,
                    uv: mesh.vertices[i1].uv,
                },
                ClipVertex {
                    position: c2,
                    shade: shade2,
                    uv: mesh.vertices[i2].uv,
                },
            ],
            color,
            texture,
            pipeline.is_two_sided(),
            vp_w,
            vp_h,
            environment.filter(|_| shaded).map(|mut env| {
                if env.improved() {
                    let up = [i0, i1, i2]
                        .map(|i| {
                            transform::transform_normal(mesh.vertices[i].normal, &normal_mat).y
                        })
                        .into_iter()
                        .sum::<f32>()
                        / 3.0;
                    env.ambient = crate::optimized_lighting::ambient(env.ambient, up);
                }
                env
            }),
            shadows.filter(|_| shaded),
        );
    }
    if authored_ui || pipeline == BasicPipelineKind::Particle {
        fb.set_depth_policy(true, true);
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
    let line = crate::api_graphic_basic::command_list::BasicLine {
        start,
        end,
        color,
        width,
        depth_bias,
    };
    let Some(line) =
        crate::api_graphic_basic::line_projection::prepare_line(&line, view_proj, [vp_w, vp_h])
    else {
        return;
    };
    let c0 = *view_proj * line.start.extend(1.0);
    let c1 = *view_proj * line.end.extend(1.0);

    let x0 = (c0.x / c0.w + 1.0) * 0.5 * vp_w;
    let y0 = (1.0 - c0.y / c0.w) * 0.5 * vp_h;
    let z0 = crate::api_graphic_basic::line_projection::biased_depth(c0.z / c0.w, depth_bias);

    let x1 = (c1.x / c1.w + 1.0) * 0.5 * vp_w;
    let y1 = (1.0 - c1.y / c1.w) * 0.5 * vp_h;
    let z1 = crate::api_graphic_basic::line_projection::biased_depth(c1.z / c1.w, depth_bias);

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

fn surface_edge_color(base_color: [u8; 4]) -> [u8; 4] {
    [
        base_color[0].saturating_sub(70),
        base_color[1].saturating_sub(70),
        base_color[2].saturating_sub(70),
        255,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_config::RenderConfig;

    #[test]
    fn plane_two_sided_rasterization_accepts_both_windings() {
        let vertices = [
            ClipVertex {
                position: Vec4::new(-0.8, -0.8, 0.0, 1.0),
                shade: 1.0,
                uv: [0.0, 0.0],
            },
            ClipVertex {
                position: Vec4::new(0.8, -0.8, 0.0, 1.0),
                shade: 1.0,
                uv: [1.0, 0.0],
            },
            ClipVertex {
                position: Vec4::new(0.8, 0.8, 0.0, 1.0),
                shade: 1.0,
                uv: [1.0, 1.0],
            },
        ];

        let back_face = [vertices[0], vertices[2], vertices[1]];
        let mut culled_framebuffer = Framebuffer::new(32, 32);
        culled_framebuffer.clear(0, 0, 0, 255);
        rasterize_clipped_triangle(
            &mut culled_framebuffer,
            back_face,
            [255, 64, 32, 255],
            None,
            false,
            32.0,
            32.0,
            None,
            None,
        );
        assert!(!culled_framebuffer
            .pixels()
            .chunks_exact(4)
            .any(|pixel| pixel[0] == 255));

        for triangle in [vertices, back_face] {
            let mut framebuffer = Framebuffer::new(32, 32);
            framebuffer.clear(0, 0, 0, 255);
            rasterize_clipped_triangle(
                &mut framebuffer,
                triangle,
                [255, 64, 32, 255],
                None,
                true,
                32.0,
                32.0,
                None,
                None,
            );

            assert!(framebuffer
                .pixels()
                .chunks_exact(4)
                .any(|pixel| pixel[0] == 255));
        }
    }

    #[test]
    fn adaptive_geometry_lod_hysteresis_prevents_boundary_chatter() {
        assert_eq!(adaptive_geometry_lod(17.0, None), 0);
        assert_eq!(adaptive_geometry_lod(19.0, Some(0)), 1);
        assert_eq!(adaptive_geometry_lod(15.0, Some(1)), 1);
        assert_eq!(adaptive_geometry_lod(11.0, Some(1)), 0);
        assert_eq!(adaptive_geometry_lod(74.0, Some(1)), 2);
        assert_eq!(adaptive_geometry_lod(64.0, Some(2)), 2);
        assert_eq!(adaptive_geometry_lod(55.0, Some(2)), 1);
    }

    #[test]
    fn geometry_detail_modes_change_curved_mesh_work() {
        let mut scene = SceneGraph::new();
        scene.add_root_with_primitive("Sphere", Primitive::Sphere);
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);

        let render_triangles = |renderer: &mut SceneRenderer, detail: GeometryDetailMode| {
            renderer
                .build_frame(
                    &scene,
                    &camera,
                    320.0,
                    240.0,
                    &[],
                    [20, 20, 20, 255],
                    Vec3::Y,
                    RenderOptions {
                        show_grid_3d: false,
                        geometry_detail: detail,
                        ..RenderOptions::default()
                    },
                    None,
                )
                .stats
                .triangles_rendered
        };

        let efficient = render_triangles(&mut renderer, GeometryDetailMode::Efficient);
        let adaptive = render_triangles(&mut renderer, GeometryDetailMode::Adaptive);
        let detailed = render_triangles(&mut renderer, GeometryDetailMode::Detailed);
        assert_eq!(efficient, (8 - 1) * 12 * 2);
        assert_eq!(detailed, (24 - 1) * 36 * 2);
        assert!(adaptive >= efficient && adaptive <= detailed);
    }

    #[test]
    fn lightweight_scene_submission_keeps_all_visible_geometry() {
        let mut scene = SceneGraph::new();
        for index in 0..4 {
            scene.add_root_with_primitive(&format!("Sphere {index}"), Primitive::Sphere);
        }
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);
        let expected_triangles = renderer.sphere_detail.medium().mesh.triangle_count() as u32 * 4;

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
    fn stable_scene_packets_ignore_camera_culling_and_lod_changes() {
        let mut scene = SceneGraph::new();
        for index in 0..32 {
            let id = scene.add_root_with_primitive(&format!("Cube {index}"), Primitive::Cube);
            if let Some(node) = scene.get_mut(id) {
                let angle = index as f32 / 32.0 * std::f32::consts::TAU;
                node.position = Vec3::new(angle.cos() * 8.0, index as f32 * 0.2, angle.sin() * 8.0);
            }
        }
        let options = RenderOptions {
            stable_scene_packet: true,
            show_grid_3d: false,
            selection_outline: false,
            ..RenderOptions::default()
        };
        let mut renderer = SceneRenderer::new(320, 240);
        let first_camera = Camera {
            position: Vec3::new(0.0, 2.0, 12.0),
            ..Camera::default()
        };
        let second_camera = Camera {
            position: Vec3::new(8.0, 3.0, 9.0),
            ..Camera::default()
        };
        let first = renderer.build_frame(
            &scene,
            &first_camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            options.clone(),
            None,
        );
        let second = renderer.build_frame(
            &scene,
            &second_camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            options,
            None,
        );
        assert_eq!(
            first.commands.content_hash(),
            second.commands.content_hash(),
            "stable packets must be camera independent"
        );
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
    fn shared_scene_environment_clears_empty_views_and_still_renders_geometry() {
        let mut scene = SceneGraph::new();
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(64, 64);
        let options = RenderOptions {
            show_grid_3d: false,
            selection_outline: false,
            ..RenderOptions::default()
        };
        let pixels = renderer.render(
            &scene,
            &camera,
            64.0,
            64.0,
            &[],
            SCENE_BACKGROUND,
            SCENE_LIGHT_DIRECTION,
            options,
            None,
        );
        assert!(pixels
            .chunks_exact(4)
            .all(|pixel| pixel == SCENE_BACKGROUND));
        scene.add_root_with_primitive("Cube", Primitive::Cube);
        let frame = renderer.build_frame(
            &scene,
            &camera,
            64.0,
            64.0,
            &[],
            SCENE_BACKGROUND,
            SCENE_LIGHT_DIRECTION,
            options,
            None,
        );
        assert!(matches!(
            frame.commands.commands().first(),
            Some(GraphicCommand::Clear {
                r: 240,
                g: 240,
                b: 242,
                a: 255
            })
        ));
        assert_eq!(frame.stats.visible_entities, 1);
        assert!(frame.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawMesh { .. } | GraphicCommand::DrawMeshBatch { .. }
        )));
    }

    #[test]
    fn base_color_textures_enter_through_the_budgeted_ingress() {
        let root = std::env::temp_dir().join(format!(
            "raf-scene-texture-ingress-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create temporary asset root");
        image::RgbaImage::from_pixel(8, 8, image::Rgba([200, 40, 20, 255]))
            .save(root.join("base.png"))
            .expect("write temporary texture");

        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Cube", Primitive::Cube);
        scene.get_mut(cube).unwrap().base_color_texture = Some("base.png".to_string());
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(64, 64);
        renderer.set_asset_root(Some(root.clone()));
        let options = RenderOptions {
            show_grid_3d: false,
            ..RenderOptions::default()
        };
        let build = |renderer: &mut SceneRenderer, scene: &SceneGraph| {
            renderer.build_frame(
                scene,
                &camera,
                64.0,
                64.0,
                &[],
                [20, 20, 20, 255],
                Vec3::Y,
                options,
                None,
            )
        };

        let first = build(&mut renderer, &scene);
        let texture_id = first
            .commands
            .commands()
            .iter()
            .find_map(|command| match command {
                GraphicCommand::DrawMesh {
                    texture_id: Some(texture_id),
                    ..
                } => Some(*texture_id),
                _ => None,
            })
            .expect("the cube should render with its base-color texture");
        let texture = first
            .commands
            .texture(texture_id)
            .expect("the texture should be registered for the frame");
        assert_eq!((texture.width, texture.height), (8, 8));
        assert!(texture
            .source
            .starts_with(root.canonicalize().expect("canonical asset root")));

        let metrics = renderer.texture_ingress_metrics();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.entries, 1);

        let _ = build(&mut renderer, &scene);
        let metrics = renderer.texture_ingress_metrics();
        assert_eq!(metrics.misses, 1);
        assert_eq!(metrics.hits, 1);
        assert_eq!(metrics.entries, 1);

        scene.get_mut(cube).unwrap().base_color_texture = Some("../base.png".to_string());
        let escaped = build(&mut renderer, &scene);
        assert!(!escaped.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawMesh {
                texture_id: Some(_),
                ..
            }
        )));
        let _ = std::fs::remove_dir_all(root);
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

    #[test]
    fn new_primitives_render_their_cached_meshes_and_bounds() {
        let renderer = SceneRenderer::new(320, 240);
        assert_eq!(
            renderer.local_bounds_for_primitive(Primitive::Wedge),
            primitives::wedge(1).aabb()
        );
        assert_eq!(
            renderer.local_bounds_for_primitive(Primitive::Torus),
            primitives::torus(32, 16).aabb()
        );
        assert_eq!(
            renderer.local_bounds_for_primitive(Primitive::Prism),
            primitives::prism(24).aabb()
        );
        assert_eq!(
            renderer.local_bounds_for_primitive(Primitive::Sphere2D),
            primitives::sphere2d(32, 16).aabb()
        );

        let mut scene = SceneGraph::new();
        for primitive in [
            Primitive::Wedge,
            Primitive::Torus,
            Primitive::Prism,
            Primitive::Sphere2D,
        ] {
            scene.add_root_with_primitive(primitive.label(), primitive);
        }
        let expected_triangles = primitives::wedge(1).triangle_count()
            + primitives::torus(32, 16).triangle_count()
            + primitives::prism(24).triangle_count()
            + primitives::sphere2d(32, 16).triangle_count();
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);
        let frame = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions {
                show_grid_3d: false,
                ..RenderOptions::default()
            },
            None,
        );

        assert_eq!(frame.stats.visible_entities, 4);
        assert_eq!(frame.stats.triangles_rendered as usize, expected_triangles);
    }

    #[test]
    fn sphere2d_billboards_toward_the_camera_and_renders_two_sided() {
        let position = Vec3::new(2.0, -1.0, 0.5);
        let mut scene = SceneGraph::new();
        let sphere2d = scene.add_root_with_primitive("Sphere 2D", Primitive::Sphere2D);
        scene.get_mut(sphere2d).unwrap().position = position;
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(320, 240);
        let frame = renderer.build_frame(
            &scene,
            &camera,
            320.0,
            240.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions {
                show_grid_3d: false,
                ..RenderOptions::default()
            },
            None,
        );

        let mut pipeline = None;
        let mut drawn = None;
        for command in frame.commands.commands() {
            match command {
                GraphicCommand::SetPipeline(kind) => pipeline = Some(*kind),
                GraphicCommand::DrawMesh { transform, .. } => {
                    drawn = Some((pipeline, *transform));
                }
                GraphicCommand::DrawMeshBatch { instances, .. } => {
                    if let Some(instance) = instances.first() {
                        drawn = Some((pipeline, instance.transform));
                    }
                }
                _ => {}
            }
        }
        let (pipeline, transform) = drawn.expect("Sphere2D should be drawn");
        assert!(matches!(
            pipeline,
            Some(BasicPipelineKind::PbrLitTwoSided | BasicPipelineKind::FlatColorTwoSided)
        ));
        assert!((transform.w_axis.truncate() - position).length() < 1e-5);
        let forward = (camera.position - position).normalize();
        assert!((transform.z_axis.truncate().normalize() - forward).length() < 1e-5);
        let up = transform.y_axis.truncate().normalize();
        assert!(up.dot(Vec3::Y) > 0.0);
        assert!(up.dot(forward).abs() < 1e-5);
    }

    fn last_drawn_transform(frame: &SceneRenderFrame) -> Option<Mat4> {
        let mut transform = None;
        for command in frame.commands.commands() {
            match command {
                GraphicCommand::DrawMesh {
                    transform: draw, ..
                } => transform = Some(*draw),
                GraphicCommand::DrawMeshBatch { instances, .. } => {
                    if let Some(instance) = instances.first() {
                        transform = Some(instance.transform);
                    }
                }
                _ => {}
            }
        }
        transform
    }

    #[test]
    fn surface_unlit_and_double_sided_select_the_flat_two_sided_pipeline() {
        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Unlit Cube", Primitive::Cube);
        {
            let node = scene.get_mut(cube).unwrap();
            node.surface.unlit = true;
            node.surface.double_sided = true;
        }
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(128, 128);
        let frame = renderer.build_frame(
            &scene,
            &camera,
            128.0,
            128.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions {
                show_grid_3d: false,
                ..RenderOptions::default()
            },
            None,
        );

        let mut pipeline = None;
        let mut drawn_pipeline = None;
        for command in frame.commands.commands() {
            match command {
                GraphicCommand::SetPipeline(kind) => pipeline = Some(*kind),
                GraphicCommand::DrawMesh { .. } | GraphicCommand::DrawMeshBatch { .. } => {
                    drawn_pipeline = pipeline;
                }
                _ => {}
            }
        }
        assert_eq!(drawn_pipeline, Some(BasicPipelineKind::FlatColorTwoSided));
    }

    #[test]
    fn base_pivot_shifts_draw_geometry_without_moving_the_authored_origin() {
        let mut scene = SceneGraph::new();
        let cube = scene.add_root_with_primitive("Pivot Cube", Primitive::Cube);
        scene.get_mut(cube).unwrap().pivot = raf_core::scene::PivotMode::Base;
        let camera = Camera::default();
        let mut renderer = SceneRenderer::new(128, 128);
        let frame = renderer.build_frame(
            &scene,
            &camera,
            128.0,
            128.0,
            &[],
            [20, 20, 20, 255],
            Vec3::Y,
            RenderOptions {
                show_grid_3d: false,
                ..RenderOptions::default()
            },
            None,
        );

        assert!(
            (scene.world_matrix(cube).w_axis.truncate() - Vec3::ZERO).length() < 1e-6,
            "the authored node transform must keep the pivot origin"
        );
        let transform = last_drawn_transform(&frame).expect("cube should be drawn");
        assert!(
            (transform.w_axis.truncate() - Vec3::new(0.0, 0.5, 0.0)).length() < 1e-5,
            "unexpected drawn translation {:?}",
            transform.w_axis.truncate()
        );
    }

    #[test]
    fn primitive_tiling_cache_scales_uvs_and_stays_bounded() {
        let renderer = SceneRenderer::new(64, 64);
        let base = Arc::clone(&renderer.cube_basic_mesh);
        let mut cache = HashMap::new();
        let key = PrimitiveTilingKey {
            primitive: primitive_batch_key(Primitive::Cube),
            lod: 1,
            tiling: [2.0_f32.to_bits(), 3.0_f32.to_bits()],
        };
        let tiled = tiled_primitive_basic(&mut cache, key, &base, [2.0, 3.0]);
        assert_eq!(tiled.vertices.len(), base.vertices.len());
        assert!(tiled
            .vertices
            .iter()
            .zip(&base.vertices)
            .all(
                |(tiled, base)| (tiled.uv[0] - base.uv[0] * 2.0).abs() < 1e-6
                    && (tiled.uv[1] - base.uv[1] * 3.0).abs() < 1e-6
            ));

        let cached = tiled_primitive_basic(&mut cache, key, &base, [2.0, 3.0]);
        assert!(Arc::ptr_eq(&tiled, &cached));

        for index in 0..(MAX_PRIMITIVE_TILING_CACHE_ENTRIES + 8) {
            let key = PrimitiveTilingKey {
                primitive: index as u8,
                lod: 1,
                tiling: [2.0_f32.to_bits(), 3.0_f32.to_bits()],
            };
            let _ = tiled_primitive_basic(&mut cache, key, &base, [2.0, 3.0]);
            assert!(cache.len() <= MAX_PRIMITIVE_TILING_CACHE_ENTRIES);
        }
    }
}
