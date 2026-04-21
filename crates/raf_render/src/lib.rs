//! # raf_render
//!
//! 2D/3D rendering engine for AuraRafi.
//! Layered architecture: SceneGraph -> RenderAbstraction -> Backend.
//!
//! Core: CPU projection + egui painter (zero GPU, runs on any hardware).
//! Prepared: wgpu pipeline, PBR materials, RT, GPU deformation, world streaming.
//! All advanced features are zero-cost when disabled (potato mode unaffected).
//!
//! v0.7.0: Added configurable render features with opt-in toggles.
//! Default = potato mode. GPU-dependent features only activate when enabled
//! AND the hardware supports them. Engine startup time is NOT affected.

// --- Core pipeline (active today, CPU painter) ---
pub mod backend;
pub mod camera;
pub mod depth_sort;
pub mod editable;
pub mod gizmo;
#[path = "ApiGraphicBasic/mod.rs"]
pub mod api_graphic_basic;
pub mod lod;
pub mod mesh;
pub mod picking;
pub mod pipeline;
pub mod projection;
pub mod renderer;

// --- v0.7.0: Advanced rendering (opt-in, zero-cost when disabled) ---
pub mod render_config;
pub mod lighting;
pub mod texture;
pub mod post_process;
pub mod shaders;
pub mod uv_mapping;

// --- v0.8.0: Software Z-buffer rasterizer (opt-in, zero-cost when disabled) ---
pub mod software_raster;

// --- Render abstraction layer (prepared, connects scene to backend) ---
pub mod abstraction;
pub mod scene_data;
pub mod material;
pub mod spatial;

// --- Advanced complements (prepared, zero cost when disabled) ---
pub mod complements;
pub mod gpu_deform;
pub mod world_stream;

// --- Core re-exports ---
pub use backend::{BackendConfig, RenderBackend, FrameRenderStats};
pub use camera::{Camera, CameraMode};
pub use depth_sort::{DepthSorter, SortableFace};
pub use editable::EditableMesh;
pub use gizmo::GizmoState;
pub use lod::LodConfig;
pub use picking::{pick_entity, pick_gizmo_arrow, project_gizmo_arrow, PickResult, GizmoScreenArrow, GIZMO_ARROWS, GIZMO_LINE_WIDTH};
pub use pipeline::RenderPipeline;
pub use renderer::Renderer;

// --- v0.7.0 re-exports ---
pub use render_config::{RenderConfig, AntiAliasingMode};
pub use lighting::{Light, LightingEnv, compute_lighting, apply_fog, bloom_factor};
pub use texture::{CpuTexture, TextureCache};
pub use post_process::{fxaa_edge_blend, apply_bloom, apply_vignette, tonemap_reinhard, adjust_saturation};
pub use uv_mapping::{UvProjection, generate_uv_box, generate_uv_spherical, generate_uv_cylindrical, cube_uv_quads};

// --- v0.8.0 re-exports ---
pub use software_raster::{SoftwareFramebuffer, RasterTriangle, rasterize_triangle, rasterize_quad, rasterize_line, rasterize_selection_outline, project_quad_for_raster, project_point_for_raster};

// --- Abstraction re-exports ---
pub use abstraction::{ActiveBackend, RenderBackendTrait, RenderCapability, RenderError};
pub use scene_data::{SceneRenderData, RenderMesh, RenderLight, RenderCamera, RenderOutput};
pub use material::{Material, MaterialLibrary, MaterialPhysics, AlphaMode};
pub use spatial::{SpatialGrid, SpatialConfig, Frustum};

// --- Complement re-exports ---
pub use complements::{RayTraceConfig, RayTraceMode, RayTraceFeatures, AccelerationStructure, Ray, RayHit};
pub use gpu_deform::{GpuDeformer, GpuDeformConfig, DeformerType};
pub use world_stream::{WorldStreamConfig, WorldStreamState, WorldRegion, BiomeType};

