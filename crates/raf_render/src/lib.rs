//! # raf_render
//!
//! 2D/3D rendering engine for AuraRafi, built on wgpu.
//! Supports adaptive quality levels from "potato" (level 0) to high-end (level 3).

pub mod camera;
pub mod mesh;
pub mod pipeline;
pub mod projection;
pub mod renderer;

pub use camera::{Camera, CameraMode};
pub use pipeline::RenderPipeline;
pub use renderer::Renderer;
