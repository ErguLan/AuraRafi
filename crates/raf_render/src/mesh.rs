//! Lightweight wireframe mesh data for viewport rendering.
//!
//! These are just arrays of edges (line segments) that define the wireframe
//! of each primitive. No GPU buffers, no allocations at render time - just
//! static vertex data that gets projected onto the 2D painter.

use glam::Vec3;

/// An edge defined by two 3D points.
pub type Edge = [Vec3; 2];

/// Get the wireframe edges for a unit cube (1x1x1 centered at origin).
pub fn cube_edges() -> Vec<Edge> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, -h, -h), Vec3::new( h, -h, -h),
        Vec3::new( h,  h, -h), Vec3::new(-h,  h, -h),
        Vec3::new(-h, -h,  h), Vec3::new( h, -h,  h),
        Vec3::new( h,  h,  h), Vec3::new(-h,  h,  h),
    ];
    vec![
        [c[0], c[1]], [c[1], c[2]], [c[2], c[3]], [c[3], c[0]],
        [c[4], c[5]], [c[5], c[6]], [c[6], c[7]], [c[7], c[4]],
        [c[0], c[4]], [c[1], c[5]], [c[2], c[6]], [c[3], c[7]],
    ]
}

/// Get face quads for a unit cube (for filled rendering).
/// Each quad is 4 corners + face normal for basic shading.
pub fn cube_faces() -> Vec<([Vec3; 4], Vec3)> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, -h, -h), Vec3::new( h, -h, -h),
        Vec3::new( h,  h, -h), Vec3::new(-h,  h, -h),
        Vec3::new(-h, -h,  h), Vec3::new( h, -h,  h),
        Vec3::new( h,  h,  h), Vec3::new(-h,  h,  h),
    ];
    vec![
        ([c[4], c[5], c[6], c[7]], Vec3::Z),
        ([c[1], c[0], c[3], c[2]], Vec3::NEG_Z),
        ([c[0], c[4], c[7], c[3]], Vec3::NEG_X),
        ([c[5], c[1], c[2], c[6]], Vec3::X),
        ([c[3], c[7], c[6], c[2]], Vec3::Y),
        ([c[0], c[1], c[5], c[4]], Vec3::NEG_Y),
    ]
}

/// Get wireframe edges for a sphere (3 orthogonal circles).
pub fn sphere_edges(segments: usize) -> Vec<Edge> {
    let mut edges = Vec::new();
    let r = 0.5;
    let seg = segments.max(8);

    for plane in 0..3 {
        for i in 0..seg {
            let a0 = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let a1 = ((i + 1) as f32 / seg as f32) * std::f32::consts::TAU;
            let (p0, p1) = match plane {
                0 => (
                    Vec3::new(r * a0.cos(), r * a0.sin(), 0.0),
                    Vec3::new(r * a1.cos(), r * a1.sin(), 0.0),
                ),
                1 => (
                    Vec3::new(r * a0.cos(), 0.0, r * a0.sin()),
                    Vec3::new(r * a1.cos(), 0.0, r * a1.sin()),
                ),
                _ => (
                    Vec3::new(0.0, r * a0.cos(), r * a0.sin()),
                    Vec3::new(0.0, r * a1.cos(), r * a1.sin()),
                ),
            };
            edges.push([p0, p1]);
        }
    }
    edges
}

/// Get wireframe edges for a unit plane (XZ, at Y=0).
pub fn plane_edges() -> Vec<Edge> {
    let h = 0.5;
    let c = [
        Vec3::new(-h, 0.0, -h), Vec3::new( h, 0.0, -h),
        Vec3::new( h, 0.0,  h), Vec3::new(-h, 0.0,  h),
    ];
    vec![
        [c[0], c[1]], [c[1], c[2]], [c[2], c[3]], [c[3], c[0]],
        [c[0], c[2]], // diagonal
    ]
}

/// Get wireframe edges for a cylinder (Y axis, radius 0.5, height 1).
pub fn cylinder_edges(segments: usize) -> Vec<Edge> {
    let mut edges = Vec::new();
    let r = 0.5;
    let h = 0.5;
    let seg = segments.max(8);

    for i in 0..seg {
        let a0 = (i as f32 / seg as f32) * std::f32::consts::TAU;
        let a1 = ((i + 1) as f32 / seg as f32) * std::f32::consts::TAU;

        let top0 = Vec3::new(r * a0.cos(), h, r * a0.sin());
        let top1 = Vec3::new(r * a1.cos(), h, r * a1.sin());
        let bot0 = Vec3::new(r * a0.cos(), -h, r * a0.sin());
        let bot1 = Vec3::new(r * a1.cos(), -h, r * a1.sin());

        edges.push([top0, top1]);
        edges.push([bot0, bot1]);
        if i % (seg / 4).max(1) == 0 {
            edges.push([bot0, top0]);
        }
    }
    edges
}
