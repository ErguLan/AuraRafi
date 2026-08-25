//! Visual design primitives for renderer-owned transform gizmos.
//!
//! Picking intentionally lives in `picking.rs`. This module only describes
//! how a projected gizmo should look, which keeps interaction tolerances
//! independent from the visual profile and makes future arrow styles safe to
//! add without rewriting the viewport controller.

use crate::api_graphic_basic::command_list::BasicScreenTriangle;
use crate::picking::GizmoScreenArrow;

/// Tunable visual profile for a translation arrowhead.
///
/// The values are in target pixels because the arrowhead is a billboard. The
/// shaft remains a world-space line, while this profile keeps the visible tip
/// readable at different camera distances and projections.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoVisualProfile {
    /// Arrow shaft width passed to the world-line renderer.
    pub shaft_width: f32,
    /// Length of the screen-facing arrowhead.
    pub head_length: f32,
    /// Width of the screen-facing arrowhead.
    pub head_width: f32,
    /// Extra pixels used by the dark underlay for contrast.
    pub outline_expand: f32,
    /// Alpha of the dark underlay.
    pub outline_alpha: u8,
}

impl GizmoVisualProfile {
    /// The default authoring profile: compact, readable and close to the
    /// familiar DCC/game-editor billboard gizmo style.
    pub const fn translation() -> Self {
        Self {
            shaft_width: 2.75,
            head_length: 24.0,
            head_width: 18.0,
            outline_expand: 2.0,
            outline_alpha: 180,
        }
    }

    fn clamped(self) -> Self {
        Self {
            shaft_width: self.shaft_width.clamp(1.0, 8.0),
            head_length: self.head_length.clamp(10.0, 48.0),
            head_width: self.head_width.clamp(8.0, 42.0),
            outline_expand: self.outline_expand.clamp(0.0, 8.0),
            outline_alpha: self.outline_alpha,
        }
    }
}

/// Return the billboard triangles that make up one arrowhead.
///
/// The outer triangle is a small dark silhouette and the inner triangle is
/// the colored face. Keeping the profile explicitly triangular makes the
/// direction readable at a glance and avoids the old rounded/blob-shaped
/// endpoints.
pub fn arrowhead_triangles(
    arrow: &GizmoScreenArrow,
    color: [u8; 4],
    profile: GizmoVisualProfile,
) -> Vec<BasicScreenTriangle> {
    let profile = profile.clamped();
    let dx = arrow.end[0] - arrow.start[0];
    let dy = arrow.end[1] - arrow.start[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length < 2.0 {
        return Vec::new();
    }

    let direction = [dx / length, dy / length];
    let perpendicular = [-direction[1], direction[0]];
    let head_length = profile.head_length.min(length * 0.42).max(8.0);
    let outer = arrowhead_polygon(
        arrow.end,
        direction,
        perpendicular,
        head_length,
        profile.head_width + profile.outline_expand * 2.0,
    );
    let inner = arrowhead_polygon(
        [
            arrow.end[0] - direction[0] * profile.outline_expand,
            arrow.end[1] - direction[1] * profile.outline_expand,
        ],
        direction,
        perpendicular,
        (head_length - profile.outline_expand).max(6.0),
        profile.head_width,
    );

    let mut triangles = fan_triangles(&outer, [20, 24, 32, profile.outline_alpha.min(color[3])]);
    triangles.extend(fan_triangles(&inner, color));
    triangles
}

/// Add a billboard arrowhead directly to the backend-neutral command list.
pub fn append_arrowhead(
    commands: &mut crate::api_graphic_basic::command_list::BasicCommandList,
    arrow: &GizmoScreenArrow,
    color: [u8; 4],
    profile: GizmoVisualProfile,
) {
    for triangle in arrowhead_triangles(arrow, color, profile) {
        commands.draw_screen_triangle(triangle);
    }
}

fn arrowhead_polygon(
    tip: [f32; 2],
    direction: [f32; 2],
    perpendicular: [f32; 2],
    length: f32,
    width: f32,
) -> [[f32; 2]; 3] {
    let base = [
        tip[0] - direction[0] * length,
        tip[1] - direction[1] * length,
    ];
    let half_width = width * 0.5;
    let left = [
        base[0] + perpendicular[0] * half_width,
        base[1] + perpendicular[1] * half_width,
    ];
    let right = [
        base[0] - perpendicular[0] * half_width,
        base[1] - perpendicular[1] * half_width,
    ];

    [tip, left, right]
}

fn fan_triangles(points: &[[f32; 2]], color: [u8; 4]) -> Vec<BasicScreenTriangle> {
    if points.len() < 3 {
        return Vec::new();
    }

    if points.len() == 3 {
        return vec![BasicScreenTriangle {
            points: [points[0], points[1], points[2]],
            color,
        }];
    }

    let center = points.iter().fold([0.0, 0.0], |sum, point| {
        [sum[0] + point[0], sum[1] + point[1]]
    });
    let center = [
        center[0] / points.len() as f32,
        center[1] / points.len() as f32,
    ];

    points
        .iter()
        .zip(points.iter().cycle().skip(1))
        .take(points.len())
        .map(|(a, b)| BasicScreenTriangle {
            points: [center, *a, *b],
            color,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected_arrow() -> GizmoScreenArrow {
        GizmoScreenArrow {
            start: [100.0, 100.0],
            end: [220.0, 100.0],
            head_tip: [220.0, 100.0],
            head_left: [200.0, 90.0],
            head_right: [200.0, 110.0],
            color: [220, 70, 70, 255],
            label: "X",
        }
    }

    #[test]
    fn translation_profile_is_bounded_and_billboard_sized() {
        let profile = GizmoVisualProfile::translation();
        assert!(profile.head_length > 0.0);
        assert!(profile.head_width > profile.shaft_width);
        assert!(profile.outline_expand > 0.0);
    }

    #[test]
    fn arrowhead_builds_outline_and_fill_fans() {
        let triangles = arrowhead_triangles(
            &projected_arrow(),
            [220, 70, 70, 255],
            GizmoVisualProfile::translation(),
        );
        assert_eq!(triangles.len(), 2);
        assert!(triangles.iter().all(|triangle| triangle
            .points
            .iter()
            .flatten()
            .all(|value| value.is_finite())));
        assert!(triangles.iter().any(|triangle| triangle.color[0] == 20));
        assert!(triangles.iter().any(|triangle| triangle.color[0] == 220));
    }
}
