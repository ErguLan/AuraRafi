//! Shared line clipping policy before either private GPU or CPU execution.
use super::command_list::BasicLine;
use glam::{Mat4, Vec4};

/// Clip geometry in homogeneous coordinates BEFORE expanding its pixel width or
/// applying depth bias. X-ray disables occlusion, never geometric clipping.
pub fn prepare_line(line: &BasicLine, view_proj: &Mat4, viewport: [f32; 2]) -> Option<BasicLine> {
    if !line.start.is_finite()
        || !line.end.is_finite()
        || !view_proj.is_finite()
        || !line.width.is_finite()
        || !line.depth_bias.is_finite()
        || viewport.iter().any(|n| !n.is_finite() || *n <= 0.0)
    {
        return None;
    }
    let a = *view_proj * line.start.extend(1.0);
    let b = *view_proj * line.end.extend(1.0);
    if !a.is_finite() || !b.is_finite() {
        return None;
    }
    // Allow stroke coverage at viewport edges, not arbitrarily huge offscreen DDA work.
    let margin = [
        1.0 + 2.0 * line.width.max(1.0) / viewport[0],
        1.0 + 2.0 * line.width.max(1.0) / viewport[1],
    ];
    let distances = |p: Vec4| {
        [
            p.x + margin[0] * p.w,
            margin[0] * p.w - p.x,
            p.y + margin[1] * p.w,
            margin[1] * p.w - p.y,
            p.z,
            p.w - p.z,
            p.w - 0.0001,
        ]
    };
    let mut enter = 0.0_f32;
    let mut exit = 1.0_f32;
    for (da, db) in distances(a).into_iter().zip(distances(b)) {
        if !da.is_finite() || !db.is_finite() || (da < 0.0 && db < 0.0) {
            return None;
        }
        if (da < 0.0) != (db < 0.0) {
            let t = da / (da - db);
            if da < 0.0 {
                enter = enter.max(t);
            } else {
                exit = exit.min(t);
            }
        }
    }
    if enter > exit {
        return None;
    }
    let mut clipped = *line;
    clipped.start = line.start.lerp(line.end, enter);
    clipped.end = line.start.lerp(line.end, exit);
    (clipped.start.is_finite() && clipped.end.is_finite() && clipped.start != clipped.end)
        .then_some(clipped)
}

/// Same zero-to-one convention as the GPU vertex shader. Bias cannot clip a
/// visible stroke across near/far; it only changes its depth-test priority.
pub fn biased_depth(depth: f32, bias: f32) -> f32 {
    (depth + bias).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    #[test]
    fn crossing_lines_clip_to_zero_to_one_volume_without_bias_culling() {
        let line = BasicLine {
            start: Vec3::new(-0.5, 0.0, -0.5),
            end: Vec3::new(0.5, 0.0, 0.5),
            color: [255; 4],
            width: 2.0,
            depth_bias: -0.99,
        };
        let clipped = prepare_line(&line, &Mat4::IDENTITY, [800.0, 600.0]).unwrap();
        assert!(clipped.start.z.abs() < 1e-6);
        assert_eq!(biased_depth(clipped.start.z, line.depth_bias), 0.0);
        let hidden = BasicLine {
            end: Vec3::new(0.5, 0.0, -0.1),
            ..line
        };
        assert!(prepare_line(&hidden, &Mat4::IDENTITY, [800.0, 600.0]).is_none());
        let projection = Mat4::perspective_rh(1.0, 1.0, 0.1, 100.0);
        let crossing = BasicLine {
            start: Vec3::new(0.0, 0.0, 0.5),
            end: Vec3::new(0.1, 0.0, -2.0),
            ..line
        };
        let clipped = prepare_line(&crossing, &projection, [800.0, 600.0]).unwrap();
        assert!(clipped.start.z < -0.09 && clipped.end.z == -2.0);
    }
}
