//! Depth sorting for correct face rendering (painter's algorithm).
//!
//! Without depth sorting, faces are drawn in scene-graph order, which causes
//! objects to incorrectly overlap. This module collects ALL faces from ALL
//! entities, sorts them by depth (farthest first), then draws them in order.
//!
//! O(n log n) per frame where n = total faces. For a scene with 100 cubes
//! (600 faces), this takes microseconds. Zero GPU. Potato-friendly.

use glam::{Mat4, Vec3, Vec4};

// ---------------------------------------------------------------------------
// Sortable face
// ---------------------------------------------------------------------------

/// A face ready for depth-sorted rendering.
/// Contains pre-projected screen points and depth for sorting.
#[derive(Clone)]
pub struct SortableFace {
    /// Projected screen points (2-4 points, triangles or quads).
    pub screen_points: Vec<[f32; 2]>,
    /// Face color (RGBA premultiplied).
    pub color: [u8; 4],
    /// Average depth in clip space (higher = farther from camera).
    pub depth: f32,
    /// Whether this face has a wireframe stroke.
    pub wireframe: bool,
    /// Wireframe color if wireframe is true.
    pub wire_color: [u8; 4],
    /// Wireframe line width.
    pub wire_width: f32,
}

fn average_depth(depths: &[f32]) -> f32 {
    depths.iter().copied().sum::<f32>() / depths.len() as f32
}

// ---------------------------------------------------------------------------
// Face collector
// ---------------------------------------------------------------------------

/// Collects faces from all entities for depth-sorted rendering.
#[derive(Default)]
pub struct DepthSorter {
    faces: Vec<SortableFace>,
}

impl DepthSorter {
    pub fn new() -> Self {
        Self { faces: Vec::with_capacity(512) }
    }

    /// Clear all faces (call at the start of each frame).
    pub fn clear(&mut self) {
        self.faces.clear();
    }

    /// Add a quad face (4 world-space corners + normal) to the sorter.
    /// Performs model transform, view-projection, perspective divide,
    /// back-face culling, and computes depth - all in one pass.
    ///
    /// Returns true if the face was added (visible), false if culled.
    pub fn add_quad(
        &mut self,
        corners: &[Vec3; 4],
        _normal: Vec3,
        model: &Mat4,
        view_proj: &Mat4,
        vp_w: f32,
        vp_h: f32,
        color: [u8; 4],
        wireframe: bool,
        wire_color: [u8; 4],
        wire_width: f32,
    ) -> bool {
        // Transform corners to clip space and project.
        let mut screen_pts: Vec<[f32; 2]> = Vec::with_capacity(4);
        let mut total_depth: f32 = 0.0;
        let mut clip_points: Vec<Vec4> = Vec::with_capacity(4);

        for corner in corners {
            let world = (*model * corner.extend(1.0)).truncate();
            let clip = *view_proj * Vec4::new(world.x, world.y, world.z, 1.0);

            // Behind camera - cull entire face.
            if clip.w <= 0.001 {
                return false;
            }

            clip_points.push(clip);

            // Perspective divide -> NDC.
            let ndc_x = clip.x / clip.w;
            let ndc_y = clip.y / clip.w;

            // NDC to screen coordinates.
            let sx = (ndc_x + 1.0) * 0.5 * vp_w;
            let sy = (1.0 - ndc_y) * 0.5 * vp_h;

            screen_pts.push([sx, sy]);
            total_depth += clip.z / clip.w;
        }

        if screen_pts.len() < 3 {
            return false;
        }

        // Back-face culling (2D cross product of first 3 screen points).
        let v1x = screen_pts[1][0] - screen_pts[0][0];
        let v1y = screen_pts[1][1] - screen_pts[0][1];
        let v2x = screen_pts[2][0] - screen_pts[0][0];
        let v2y = screen_pts[2][1] - screen_pts[0][1];
        let cross = v1x * v2y - v1y * v2x;
        if cross < 0.0 {
            return false;
        }

        // Deduplicate consecutive identical points (degenerate quads from caps).
        let mut deduped: Vec<[f32; 2]> = Vec::with_capacity(4);
        for p in &screen_pts {
            let dominated = deduped.last().map(|last| {
                let dx = last[0] - p[0];
                let dy = last[1] - p[1];
                (dx * dx + dy * dy) < 0.25
            }).unwrap_or(false);
            if !dominated {
                deduped.push(*p);
            }
        }

        if deduped.len() < 3 {
            return false;
        }

        if deduped.len() == 3 {
            self.faces.push(SortableFace {
                screen_points: deduped,
                color,
                depth: average_depth(&[
                    total_depth / screen_pts.len() as f32,
                    total_depth / screen_pts.len() as f32,
                    total_depth / screen_pts.len() as f32,
                ]),
                wireframe,
                wire_color,
                wire_width,
            });
            return true;
        }

        if deduped.len() == 4 {
            let tri_a_depth = average_depth(&[
                clip_points[0].z / clip_points[0].w,
                clip_points[1].z / clip_points[1].w,
                clip_points[2].z / clip_points[2].w,
            ]);
            let tri_b_depth = average_depth(&[
                clip_points[0].z / clip_points[0].w,
                clip_points[2].z / clip_points[2].w,
                clip_points[3].z / clip_points[3].w,
            ]);

            self.faces.push(SortableFace {
                screen_points: vec![deduped[0], deduped[1], deduped[2]],
                color,
                depth: tri_a_depth,
                wireframe,
                wire_color,
                wire_width,
            });

            self.faces.push(SortableFace {
                screen_points: vec![deduped[0], deduped[2], deduped[3]],
                color,
                depth: tri_b_depth,
                wireframe,
                wire_color,
                wire_width,
            });

            return true;
        }

        let avg_depth = total_depth / screen_pts.len() as f32;

        self.faces.push(SortableFace {
            screen_points: deduped,
            color,
            depth: avg_depth,
            wireframe,
            wire_color,
            wire_width,
        });

        true
    }

    /// Sort all faces by depth (farthest first = painter's algorithm).
    pub fn sort(&mut self) {
        self.faces.sort_unstable_by(|a, b| {
            b.depth.partial_cmp(&a.depth).unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    /// Get sorted faces for drawing.
    pub fn faces(&self) -> &[SortableFace] {
        &self.faces
    }

    /// Total face count (for stats).
    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    /// Total triangle count (faces with 3 pts = 1 tri, 4 pts = 2 tris).
    pub fn triangle_count(&self) -> usize {
        self.faces.iter().map(|f| {
            if f.screen_points.len() >= 4 { 2 } else { 1 }
        }).sum()
    }
}

// ---------------------------------------------------------------------------
// Brightness calculation (moved here for reuse)
// ---------------------------------------------------------------------------

/// Calculate directional light shading (dot product).
/// Returns a brightness factor from 0.3 (shadow) to 1.0 (fully lit).
pub fn face_brightness(face_normal: Vec3, light_dir: Vec3, model: &Mat4) -> f32 {
    let world_normal = (*model * Vec4::from((face_normal, 0.0))).truncate().normalize();
    let dot = world_normal.dot(light_dir.normalize());
    0.3 + 0.7 * dot.max(0.0)
}

/// Apply brightness to a base color. Returns RGBA premultiplied.
pub fn shade_color(r: u8, g: u8, b: u8, alpha: u8, brightness: f32) -> [u8; 4] {
    [
        (r as f32 * brightness).min(255.0) as u8,
        (g as f32 * brightness).min(255.0) as u8,
        (b as f32 * brightness).min(255.0) as u8,
        alpha,
    ]
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_sorter() {
        let sorter = DepthSorter::new();
        assert_eq!(sorter.face_count(), 0);
        assert_eq!(sorter.triangle_count(), 0);
    }

    #[test]
    fn shade_color_brightness() {
        let c = shade_color(200, 100, 50, 180, 0.5);
        assert_eq!(c[0], 100);
        assert_eq!(c[1], 50);
        assert_eq!(c[2], 25);
        assert_eq!(c[3], 180);
    }

    #[test]
    fn shade_color_clamp() {
        let c = shade_color(255, 255, 255, 255, 1.0);
        assert_eq!(c[0], 255);
    }
}
