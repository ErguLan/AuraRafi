//! Visual design primitives for renderer-owned transform gizmos.
//!
//! Picking intentionally lives in `picking.rs`. This module only describes
//! how a projected gizmo should look, which keeps interaction tolerances
//! independent from the visual profile and makes future arrow styles safe to
//! add without rewriting the viewport controller.

use crate::api_graphic_basic::command_list::BasicScreenTriangle;
use crate::picking::GizmoScreenArrow;

/// Tunable visual profile for a translation arrow.
///
/// Every value is a screen-space quantity. Arrow geometry is screen-space too,
/// so a shaft and its head can never drift apart while the camera moves: the
/// profile only decides proportions, never world units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoVisualProfile {
    /// Shaft thickness in pixels.
    pub shaft_width: f32,
    /// Head length as a fraction of the projected arrow length.
    pub head_length_ratio: f32,
    /// Smallest head length in pixels, so close-up arrows stay readable.
    pub head_length_min: f32,
    /// Largest head length in pixels, so distant arrows stay clean.
    pub head_length_max: f32,
    /// Half width of the head as a fraction of its length.
    pub head_half_width_ratio: f32,
    /// Dark rim thickness in pixels drawn behind the colored geometry.
    pub outline_width: f32,
    /// Color of that rim. It is what keeps the gizmo legible over any scene.
    pub outline_color: [u8; 4],
}

impl GizmoVisualProfile {
    /// The default authoring profile: compact, readable and close to the
    /// familiar DCC/game-editor billboard gizmo style.
    pub const fn translation() -> Self {
        Self {
            shaft_width: 3.0,
            head_length_ratio: 0.34,
            head_length_min: 13.0,
            head_length_max: 30.0,
            head_half_width_ratio: 0.34,
            outline_width: 1.75,
            outline_color: [6, 8, 12, 225],
        }
    }

    fn clamped(self) -> Self {
        Self {
            shaft_width: self.shaft_width.clamp(1.5, 8.0),
            head_length_ratio: self.head_length_ratio.clamp(0.1, 0.6),
            head_length_min: self.head_length_min.clamp(6.0, 30.0),
            head_length_max: self.head_length_max.clamp(10.0, 64.0),
            head_half_width_ratio: self.head_half_width_ratio.clamp(0.15, 0.8),
            outline_width: self.outline_width.clamp(0.0, 6.0),
            outline_color: self.outline_color,
        }
    }
}

/// Resolved screen-space geometry of one arrow.
///
/// `shaft_end` is the point where the shaft stops and the head starts. Drawing
/// the shaft only up to this point is what removes the stick that used to poke
/// out through the arrowhead tip.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScreenArrowGeometry {
    pub direction: [f32; 2],
    pub perpendicular: [f32; 2],
    pub length: f32,
    pub head_length: f32,
    pub head_half_width: f32,
    pub shaft_end: [f32; 2],
}

/// Measures one projected arrow and derives every screen-space dimension the
/// renderer and the visual tests share.
pub fn screen_arrow_geometry(
    arrow: &GizmoScreenArrow,
    profile: GizmoVisualProfile,
) -> Option<ScreenArrowGeometry> {
    let profile = profile.clamped();
    let dx = arrow.head_tip[0] - arrow.start[0];
    let dy = arrow.head_tip[1] - arrow.start[1];
    let length = (dx * dx + dy * dy).sqrt();
    if length < 4.0 {
        return None;
    }
    let direction = [dx / length, dy / length];
    let perpendicular = [-direction[1], direction[0]];
    let head_length = (length * profile.head_length_ratio)
        .clamp(profile.head_length_min, profile.head_length_max)
        .min(length * 0.55);
    Some(ScreenArrowGeometry {
        direction,
        perpendicular,
        length,
        head_length,
        head_half_width: head_length * profile.head_half_width_ratio,
        shaft_end: [
            arrow.head_tip[0] - direction[0] * head_length,
            arrow.head_tip[1] - direction[1] * head_length,
        ],
    })
}

/// Two triangles covering a screen-space segment of `width` pixels.
pub fn screen_line_triangles(
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    color: [u8; 4],
) -> [BasicScreenTriangle; 2] {
    let dx = to[0] - from[0];
    let dy = to[1] - from[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 0.001 {
        return [BasicScreenTriangle {
            points: [from, from, from],
            color,
        }; 2];
    }
    let half = width * 0.5;
    let nx = -dy / len * half;
    let ny = dx / len * half;
    let a = [from[0] + nx, from[1] + ny];
    let b = [to[0] + nx, to[1] + ny];
    let c = [to[0] - nx, to[1] - ny];
    let d = [from[0] - nx, from[1] - ny];
    [
        BasicScreenTriangle {
            points: [a, b, c],
            color,
        },
        BasicScreenTriangle {
            points: [a, c, d],
            color,
        },
    ]
}

/// Append a screen-space segment as two batched triangles.
pub fn append_screen_line(
    commands: &mut crate::api_graphic_basic::command_list::BasicCommandList,
    from: [f32; 2],
    to: [f32; 2],
    width: f32,
    color: [u8; 4],
) {
    for triangle in screen_line_triangles(from, to, width, color) {
        commands.draw_screen_triangle(triangle);
    }
}

/// Triangle that makes one arrowhead tip, given a length and a half width.
pub fn arrowhead_triangle(
    tip: [f32; 2],
    direction: [f32; 2],
    perpendicular: [f32; 2],
    length: f32,
    half_width: f32,
) -> [BasicScreenTriangle; 1] {
    let base = [
        tip[0] - direction[0] * length,
        tip[1] - direction[1] * length,
    ];
    let left = [
        base[0] + perpendicular[0] * half_width,
        base[1] + perpendicular[1] * half_width,
    ];
    let right = [
        base[0] - perpendicular[0] * half_width,
        base[1] - perpendicular[1] * half_width,
    ];
    [BasicScreenTriangle {
        points: [tip, left, right],
        color: [0, 0, 0, 0],
    }]
}

/// Dark rim behind the whole arrow: a wider shaft plus a slightly larger head.
/// Drawing it first is what gives the gizmo a clean silhouette instead of the
/// ragged half-outline the old profile produced.
pub fn append_arrow_outline(
    commands: &mut crate::api_graphic_basic::command_list::BasicCommandList,
    arrow: &GizmoScreenArrow,
    profile: GizmoVisualProfile,
) -> Option<ScreenArrowGeometry> {
    let profile = profile.clamped();
    let geometry = screen_arrow_geometry(arrow, profile)?;
    let grow = profile.outline_width;
    append_screen_line(
        commands,
        arrow.start,
        geometry.shaft_end,
        profile.shaft_width + grow * 2.0,
        profile.outline_color,
    );
    let tip = [
        arrow.head_tip[0] + geometry.direction[0] * grow,
        arrow.head_tip[1] + geometry.direction[1] * grow,
    ];
    let mut triangle = arrowhead_triangle(
        tip,
        geometry.direction,
        geometry.perpendicular,
        geometry.head_length + grow,
        geometry.head_half_width + grow,
    );
    triangle[0].color = profile.outline_color;
    commands.draw_screen_triangle(triangle[0]);
    // A short collar hides the seam where the outline quad meets the head.
    append_screen_line(
        commands,
        [
            geometry.shaft_end[0] - geometry.direction[0] * grow,
            geometry.shaft_end[1] - geometry.direction[1] * grow,
        ],
        geometry.shaft_end,
        profile.shaft_width + grow * 2.0,
        profile.outline_color,
    );
    Some(geometry)
}

/// Colored shaft plus colored head, drawn on top of the outline.
pub fn append_arrow_fill(
    commands: &mut crate::api_graphic_basic::command_list::BasicCommandList,
    arrow: &GizmoScreenArrow,
    color: [u8; 4],
    profile: GizmoVisualProfile,
    geometry: ScreenArrowGeometry,
) {
    let profile = profile.clamped();
    append_screen_line(
        commands,
        arrow.start,
        geometry.shaft_end,
        profile.shaft_width,
        color,
    );
    let mut triangle = arrowhead_triangle(
        arrow.head_tip,
        geometry.direction,
        geometry.perpendicular,
        geometry.head_length,
        geometry.head_half_width,
    );
    triangle[0].color = color;
    commands.draw_screen_triangle(triangle[0]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projected_arrow() -> GizmoScreenArrow {
        GizmoScreenArrow {
            start: [100.0, 100.0],
            end: [196.0, 100.0],
            head_tip: [196.0, 100.0],
            head_left: [176.0, 90.0],
            head_right: [176.0, 110.0],
            color: [220, 70, 70, 255],
            label: "X",
        }
    }

    #[test]
    fn the_shaft_stops_at_the_head_base_so_no_stick_pokes_out() {
        let arrow = projected_arrow();
        let geometry = screen_arrow_geometry(&arrow, GizmoVisualProfile::translation()).unwrap();

        assert!(geometry.shaft_end[0] < arrow.head_tip[0]);
        // The shaft base is exactly the head base, never beyond the tip.
        assert!(
            geometry.shaft_end[0] <= arrow.head_tip[0] + f32::EPSILON,
            "shaft must not pass the arrow tip"
        );
    }

    #[test]
    fn head_size_follows_the_projected_length_within_bounds() {
        let profile = GizmoVisualProfile::translation();
        let mut short = projected_arrow();
        short.end = [140.0, 100.0];
        short.head_tip = [140.0, 100.0];
        let short_geometry = screen_arrow_geometry(&short, profile).unwrap();

        let mut long = projected_arrow();
        long.end = [600.0, 100.0];
        long.head_tip = [600.0, 100.0];
        let long_geometry = screen_arrow_geometry(&long, profile).unwrap();

        assert!(long_geometry.head_length > short_geometry.head_length);
        assert!(long_geometry.head_length <= profile.head_length_max);
        assert!(short_geometry.head_length >= profile.head_length_min.min(40.0) * 0.5);
    }

    #[test]
    fn a_degenerate_projection_yields_no_geometry() {
        let mut arrow = projected_arrow();
        arrow.head_tip = arrow.start;
        assert!(screen_arrow_geometry(&arrow, GizmoVisualProfile::translation()).is_none());
    }

    #[test]
    fn screen_lines_produce_two_triangles_at_the_requested_width() {
        let triangles = screen_line_triangles([0.0, 0.0], [10.0, 0.0], 4.0, [255, 0, 0, 255]);
        let ys: Vec<f32> = triangles
            .iter()
            .flat_map(|triangle| triangle.points.iter().map(|point| point[1]))
            .collect();
        assert_eq!(ys.len(), 6);
        let extent = ys.iter().cloned().fold(f32::NEG_INFINITY, f32::max)
            - ys.iter().cloned().fold(f32::INFINITY, f32::min);
        assert!((extent - 4.0).abs() <= f32::EPSILON, "quad height {extent}");
    }
}
