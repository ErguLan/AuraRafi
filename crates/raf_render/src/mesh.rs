//! Compatibility wrappers for primitive recipes.
//!
//! The actual primitive construction now lives in ApiGraphicBasic so the
//! renderer can pull default shapes from recipe files instead of keeping the
//! generation logic embedded here.

pub use crate::api_graphic_basic::recipes::Edge;

pub fn cube_edges() -> Vec<Edge> {
    crate::api_graphic_basic::recipes::cube_edges()
}

pub fn cube_faces() -> Vec<([glam::Vec3; 4], glam::Vec3)> {
    crate::api_graphic_basic::recipes::cube_faces()
}

pub fn sphere_edges(segments: usize) -> Vec<Edge> {
    crate::api_graphic_basic::recipes::sphere_edges(segments)
}

pub fn sphere_faces(stacks: usize, slices: usize) -> Vec<([glam::Vec3; 4], glam::Vec3)> {
    crate::api_graphic_basic::recipes::sphere_faces(stacks, slices)
}

pub fn plane_edges() -> Vec<Edge> {
    crate::api_graphic_basic::recipes::plane_edges()
}

pub fn plane_faces() -> Vec<([glam::Vec3; 4], glam::Vec3)> {
    crate::api_graphic_basic::recipes::plane_faces()
}

pub fn cylinder_edges(segments: usize) -> Vec<Edge> {
    crate::api_graphic_basic::recipes::cylinder_edges(segments)
}

pub fn cylinder_faces(segments: usize) -> Vec<([glam::Vec3; 4], glam::Vec3)> {
    crate::api_graphic_basic::recipes::cylinder_faces(segments)
}
