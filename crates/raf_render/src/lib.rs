//! # raf_render
//!
//! 2D/3D rendering engine for AuraRafi.
//!
//! Architecture (v0.9.0+):
//! - `geometry/`       - Indexed triangle mesh data and primitive constructors
//! - `math/`           - Matrix transforms, ray casting, frustum culling
//! - `render_pipeline/`- CPU scanline rasterizer with Z-buffer
//! - `scene_renderer`  - Full scene render orchestrator (scene in, pixels out)
//!
//! The editor presentation path is native Winit + RafUI. Older renderer
//! modules remain only where they still provide backend-neutral scene, math,
//! or compatibility data; they are not an alternate UI host.

// === NEW RENDERER ARCHITECTURE ===
pub mod bridge;
pub mod geometry;
pub mod math;
pub mod render_pipeline;
pub mod scene_renderer;

// --- Core renderer and compatibility exports ---
// The active editor path is ApiGraphicBasic + SceneRenderer with GPU-first
// execution and CPU recovery. The modules below include active math/picking
// contracts plus prepared or compatibility APIs; see docs/RENDERER.md for the
// current classification instead of treating every public export as active.
#[path = "ApiGraphicBasic/mod.rs"]
pub mod api_graphic_basic;
pub mod backend;
pub mod camera;
pub mod depth_sort;
pub mod editable;
pub mod gizmo;
pub mod gizmo_visual;
pub mod lod;
pub mod mesh;
pub mod picking;
pub mod picking_id_buffer;
pub mod pipeline;
pub mod projection;
pub mod renderer;

// --- Prepared advanced rendering (opt-in, zero-cost when disabled) ---
pub mod lighting;
pub mod post_process;
pub mod render_config;
pub mod shaders;
pub mod texture;
pub mod uv_mapping;

// --- CPU recovery and software rasterization ---
pub mod software_raster;

// --- Prepared render abstraction layer ---
pub mod abstraction;
pub mod material;
pub mod scene_data;
pub mod spatial;

// --- Prepared advanced complements (zero cost when disabled) ---
pub mod complements;
pub mod gpu_deform;
pub mod world_stream;

// --- Core re-exports ---
pub use backend::{BackendConfig, FrameRenderStats, RenderBackend};
pub use camera::{Camera, CameraMode};
pub use depth_sort::{DepthSorter, SortableFace};
pub use editable::EditableMesh;
pub use gizmo::GizmoState;
pub use lod::LodConfig;
pub use picking::{
    pick_entity, pick_gizmo_arrow, project_gizmo_arrow, GizmoScreenArrow, PickResult, GIZMO_ARROWS,
    GIZMO_LINE_WIDTH,
};
pub use picking_id_buffer::{PickHit, PickObjectId, PickPixel, SelectionIdBuffer};
pub use pipeline::RenderPipeline;
pub use renderer::Renderer;

// --- v0.7.0 re-exports ---
pub use lighting::{apply_fog, bloom_factor, compute_lighting, Light, LightingEnv};
pub use post_process::{
    adjust_saturation, apply_bloom, apply_vignette, fxaa_edge_blend, tonemap_reinhard,
};
pub use render_config::{AntiAliasingMode, RenderConfig, RenderResourceProfile};
pub use texture::{CpuTexture, TextureCache};
pub use uv_mapping::{
    cube_uv_quads, generate_uv_box, generate_uv_cylindrical, generate_uv_spherical, UvProjection,
};

// --- v0.8.0 re-exports ---
pub use software_raster::{
    project_point_for_raster, project_quad_for_raster, rasterize_line, rasterize_quad,
    rasterize_selection_outline, rasterize_triangle, RasterTriangle, SoftwareFramebuffer,
};

// --- Abstraction re-exports ---
pub use abstraction::{ActiveBackend, RenderBackendTrait, RenderCapability, RenderError};
pub use material::{AlphaMode, Material, MaterialLibrary, MaterialPhysics};
pub use scene_data::{RenderCamera, RenderLight, RenderMesh, RenderOutput, SceneRenderData};
pub use spatial::{Frustum, SpatialConfig, SpatialGrid};

// --- Complement re-exports ---
pub use complements::{
    AccelerationStructure, Ray, RayHit, RayTraceConfig, RayTraceFeatures, RayTraceMode,
};
pub use gpu_deform::{DeformerType, GpuDeformConfig, GpuDeformer};
pub use world_stream::{BiomeType, WorldRegion, WorldStreamConfig, WorldStreamState};
