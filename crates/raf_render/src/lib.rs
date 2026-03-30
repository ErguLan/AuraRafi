//! # raf_render
//!
//! 2D/3D rendering engine for AuraRafi.
//! CPU projection + egui painter (zero GPU pipelines, runs on any hardware).
//! Supports adaptive quality levels from "potato" (level 0) to high-end (level 3).

pub mod camera;
pub mod editable;
pub mod gizmo;
pub mod lod;
pub mod mesh;
pub mod pipeline;
pub mod projection;
pub mod renderer;

pub use camera::{Camera, CameraMode};
pub use editable::EditableMesh;
pub use gizmo::GizmoState;
pub use lod::LodConfig;
pub use pipeline::RenderPipeline;
pub use renderer::Renderer;
