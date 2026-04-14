//! # raf_render
//!
//! 2D/3D rendering engine for AuraRafi.
//! Layered architecture: SceneGraph -> RenderAbstraction -> Backend.
//!
//! Core: CPU projection + egui painter (zero GPU, runs on any hardware).
//! Prepared: wgpu pipeline, PBR materials, RT, GPU deformation, world streaming.
//! All advanced features are zero-cost when disabled (potato mode unaffected).
//!
//! Supports adaptive quality levels from "potato" (level 0) to high-end (level 3).

// --- Core pipeline (active today) ---
pub mod backend;
pub mod camera;
pub mod depth_sort;
pub mod editable;
pub mod gizmo;
pub mod lod;
pub mod mesh;
pub mod picking;
pub mod pipeline;
pub mod projection;
pub mod renderer;

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

// --- Abstraction re-exports ---
pub use abstraction::{ActiveBackend, RenderBackendTrait, RenderCapability, RenderError};
pub use scene_data::{SceneRenderData, RenderMesh, RenderLight, RenderCamera, RenderOutput};
pub use material::{Material, MaterialLibrary, MaterialPhysics, AlphaMode};
pub use spatial::{SpatialGrid, SpatialConfig, Frustum};

// --- Complement re-exports ---
pub use complements::{RayTraceConfig, RayTraceMode, RayTraceFeatures, AccelerationStructure, Ray, RayHit};
pub use gpu_deform::{GpuDeformer, GpuDeformConfig, DeformerType};
pub use world_stream::{WorldStreamConfig, WorldStreamState, WorldRegion, BiomeType};
