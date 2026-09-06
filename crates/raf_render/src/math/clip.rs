//! Allocation-free homogeneous clipping used by the CPU recovery renderer.

use glam::Vec4;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ClipVertex {
    pub position: Vec4,
    pub shade: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ClippedTriangle {
    vertices: [ClipVertex; 4],
    len: usize,
}

impl ClippedTriangle {
    pub fn vertices(&self) -> &[ClipVertex] {
        &self.vertices[..self.len]
    }
}

/// Clips one triangle against the zero-to-one near plane (`clip.z >= 0`).
/// A triangle can become a quad, which callers rasterize as a two-triangle
/// fan. This matches the GPU clip volume without allocating in the hot path.
pub fn clip_triangle_to_near(input: [ClipVertex; 3]) -> ClippedTriangle {
    let mut output = [ClipVertex::default(); 4];
    let mut output_len = 0usize;
    let mut previous = input[2];
    let mut previous_inside = previous.position.z >= 0.0;

    for current in input {
        let current_inside = current.position.z >= 0.0;
        if current_inside != previous_inside {
            let denominator = previous.position.z - current.position.z;
            if denominator.abs() > f32::EPSILON {
                let t = (previous.position.z / denominator).clamp(0.0, 1.0);
                output[output_len] = ClipVertex {
                    position: previous.position + (current.position - previous.position) * t,
                    shade: previous.shade + (current.shade - previous.shade) * t,
                };
                output_len += 1;
            }
        }
        if current_inside {
            output[output_len] = current;
            output_len += 1;
        }
        previous = current;
        previous_inside = current_inside;
    }

    ClippedTriangle {
        vertices: output,
        len: output_len,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vertex(z: f32) -> ClipVertex {
        ClipVertex {
            position: Vec4::new(0.0, 0.0, z, 1.0),
            shade: 1.0,
        }
    }

    #[test]
    fn near_clip_keeps_a_fully_visible_triangle() {
        let clipped = clip_triangle_to_near([vertex(0.1), vertex(0.2), vertex(0.3)]);

        assert_eq!(clipped.vertices().len(), 3);
    }

    #[test]
    fn near_clip_turns_one_crossing_triangle_into_a_quad() {
        let clipped = clip_triangle_to_near([vertex(-0.2), vertex(0.2), vertex(0.4)]);

        assert_eq!(clipped.vertices().len(), 4);
        assert!(clipped
            .vertices()
            .iter()
            .all(|vertex| vertex.position.z >= 0.0));
    }

    #[test]
    fn near_clip_discards_only_a_fully_hidden_triangle() {
        let clipped = clip_triangle_to_near([vertex(-0.1), vertex(-0.2), vertex(-0.3)]);

        assert!(clipped.vertices().is_empty());
    }
}
