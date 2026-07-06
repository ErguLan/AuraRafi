//! Render math utilities: matrix construction, ray casting, unprojection.
//!
//! Provides the mathematical foundation for the render pipeline:
//! - Model/View/Projection matrix construction
//! - Screen-to-world unprojection (for picking, gizmo interaction)
//! - Ray-geometry intersection tests
//! - Frustum extraction and culling
//!
//! All functions use glam types and the right-handed coordinate convention
//! (Y-up, -Z forward in view space).

pub mod frustum;
pub mod ray;
pub mod transform;
