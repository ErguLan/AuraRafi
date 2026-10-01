//! ApiGraphicBasic recording for editor transform gizmos.
//!
//! Gizmos are renderer-owned authoring geometry, not retained UI widgets.
//! Recording them into the scene command list keeps CPU/GPU recovery and the
//! native viewport visually consistent without a second painter overlay.

use glam::Vec3;

use crate::api_graphic_basic::command_list::{BasicCommandList, BasicScreenTriangle};
use crate::gizmo::{GizmoAxis, GizmoMode};
use crate::gizmo_visual::{self, GizmoVisualProfile};
use crate::picking::{
    gizmo_scale_handle_radius, project_gizmo_arrow_scaled,
    project_gizmo_scale_handles_oriented_for_axes, GIZMO_ARROWS, GIZMO_ROTATION_RADIUS,
};
use crate::scene_renderer::SceneRenderFrame;

const ACTIVE_COLOR: [u8; 4] = [255, 210, 80, 255];
const HANDLE_COLOR: [u8; 4] = [255, 190, 52, 255];
const HANDLE_OUTLINE_COLOR: [u8; 4] = [5, 7, 10, 255];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GizmoRenderSpec {
    pub mode: GizmoMode,
    pub active_axis: GizmoAxis,
    pub active_scale_sign: f32,
    pub origin: Vec3,
    pub entity_scale: Vec3,
    /// World directions of the entity's transformed local X/Y/Z axes.
    pub entity_axes: [Vec3; 3],
    /// Local axes that have visible extent and can be meaningfully scaled.
    pub enabled_scale_axes: [bool; 3],
    pub presentation_scale: f32,
}

impl GizmoRenderSpec {
    pub fn append_to(self, frame: &mut SceneRenderFrame) {
        match self.mode {
            GizmoMode::Translate => self.record_translation(frame),
            GizmoMode::Rotate => self.record_rotation(frame),
            GizmoMode::Scale => self.record_scale(frame),
        }
    }

    fn record_translation(self, frame: &mut SceneRenderFrame) {
        let profile = GizmoVisualProfile::translation();
        for (index, arrow) in GIZMO_ARROWS.iter().enumerate() {
            let axis = axis_for_index(index);
            let color = self.axis_color(axis, arrow.color);
            let Some(projected) = project_gizmo_arrow_scaled(
                self.origin,
                arrow,
                self.presentation_scale,
                &frame.view_proj,
                frame.width as f32,
                frame.height as f32,
            ) else {
                continue;
            };
            // Dark rim first, then the colored shaft and head. The shaft stops
            // at the head base, so no line can poke through the arrow tip.
            let Some(geometry) =
                gizmo_visual::append_arrow_outline(&mut frame.commands, &projected, profile)
            else {
                continue;
            };
            gizmo_visual::append_arrow_fill(
                &mut frame.commands,
                &projected,
                color,
                profile,
                geometry,
            );
        }
    }

    fn record_rotation(self, frame: &mut SceneRenderFrame) {
        let radius = GIZMO_ROTATION_RADIUS * self.presentation_scale.max(0.1);
        let planes = [(Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y)];
        for (axis_index, (axis_a, axis_b)) in planes.into_iter().enumerate() {
            let axis = axis_for_index(axis_index);
            let fallback = GIZMO_ARROWS[axis_index].color;
            let color = self.axis_color(axis, fallback);
            let active = self.active_axis == axis;
            // Two passes: a wider dark ring, then the colored one on top, so
            // the ring stays readable over bright or busy geometry.
            for (width, ring_color) in ring_passes(color, active) {
                let mut previous = self.origin + axis_a * radius;
                for step in 1..=48 {
                    let angle = (step as f32 / 48.0) * std::f32::consts::TAU;
                    let current = self.origin
                        + axis_a * (angle.cos() * radius)
                        + axis_b * (angle.sin() * radius);
                    frame
                        .commands
                        .draw_line(previous, current, ring_color, width, true, -0.99);
                    previous = current;
                }
            }
        }
    }

    fn record_scale(self, frame: &mut SceneRenderFrame) {
        let handles = project_gizmo_scale_handles_oriented_for_axes(
            self.origin,
            self.entity_scale,
            self.entity_axes,
            self.enabled_scale_axes,
            &frame.view_proj,
            frame.width as f32,
            frame.height as f32,
        );
        for handle in handles {
            let axis = axis_for_index(handle.axis_index);
            let active = axis == self.active_axis
                && (self.active_scale_sign == 0.0
                    || (self.active_scale_sign.signum() - handle.sign).abs() < 0.1);
            let color = if active { ACTIVE_COLOR } else { HANDLE_COLOR };
            let radius = gizmo_scale_handle_radius(self.presentation_scale)
                + if active { 2.0 } else { 0.0 };
            // Dark disc first, colored disc on top.
            append_screen_circle(
                &mut frame.commands,
                handle.center,
                radius + HANDLE_OUTLINE_WIDTH,
                HANDLE_OUTLINE_COLOR,
            );
            append_screen_circle(&mut frame.commands, handle.center, radius, color);
        }
    }

    fn axis_color(self, axis: GizmoAxis, fallback: [u8; 4]) -> [u8; 4] {
        if self.active_axis == axis {
            ACTIVE_COLOR
        } else {
            fallback
        }
    }
}

/// Outline thickness drawn behind rotation rings and scale handles.
const HANDLE_OUTLINE_WIDTH: f32 = 2.0;

/// Width and color passes for a rotation ring: outline first, then the ring.
fn ring_passes(color: [u8; 4], active: bool) -> [(f32, [u8; 4]); 2] {
    let ring_width = if active { 3.5 } else { 2.25 };
    [
        (ring_width + HANDLE_OUTLINE_WIDTH * 2.0, HANDLE_OUTLINE_COLOR),
        (ring_width, color),
    ]
}

/// Record one filled, screen-facing handle. The command list is shared by the
/// GPU and CPU recovery paths, so scale handles stay true billboards on both.
fn append_screen_circle(
    commands: &mut BasicCommandList,
    center: [f32; 2],
    radius: f32,
    color: [u8; 4],
) {
    // The dark outer rim separates the handle from bright geometry while the
    // center preserves the old warm billboard identity.
    append_screen_circle_fill(commands, center, radius + 4.0, HANDLE_OUTLINE_COLOR);
    append_screen_circle_fill(commands, center, radius, color);
}

fn append_screen_circle_fill(
    commands: &mut BasicCommandList,
    center: [f32; 2],
    radius: f32,
    color: [u8; 4],
) {
    const SEGMENTS: usize = 20;
    let mut previous = [center[0] + radius, center[1]];
    for segment in 1..=SEGMENTS {
        let angle = (segment as f32 / SEGMENTS as f32) * std::f32::consts::TAU;
        let current = [
            center[0] + angle.cos() * radius,
            center[1] + angle.sin() * radius,
        ];
        commands.draw_screen_triangle(BasicScreenTriangle {
            points: [center, previous, current],
            color,
        });
        previous = current;
    }
}

fn axis_for_index(index: usize) -> GizmoAxis {
    [GizmoAxis::X, GizmoAxis::Y, GizmoAxis::Z][index.min(2)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::command_list::{BasicCommandList, GraphicCommand};
    use crate::scene_renderer::{FrameStats, SceneRenderFrame};
    use glam::Mat4;

    #[test]
    fn translation_records_screen_geometry_only_so_no_shaft_can_poke_out() {
        let mut frame = SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: 800,
            height: 600,
            texture_cache_budget_bytes: 0,
            stats: FrameStats::default(),
        };

        GizmoRenderSpec {
            mode: GizmoMode::Translate,
            active_axis: GizmoAxis::None,
            active_scale_sign: 0.0,
            origin: Vec3::ZERO,
            entity_scale: Vec3::ONE,
            entity_axes: [Vec3::X, Vec3::Y, Vec3::Z],
            enabled_scale_axes: [true; 3],
            presentation_scale: 1.0,
        }
        .append_to(&mut frame);

        // Shaft and head are both screen-space now, so a world line can never
        // reach past the arrowhead tip.
        assert!(frame.commands.commands().iter().all(|command| matches!(
            command,
            GraphicCommand::DrawScreenTriangleBatch { .. }
        )));
        let triangles = frame
            .commands
            .commands()
            .iter()
            .find_map(|command| match command {
                GraphicCommand::DrawScreenTriangleBatch { triangles } => Some(triangles.len()),
                _ => None,
            })
            .unwrap_or(0);
        // Each visible axis contributes eight triangles: a dark outline shaft,
        // a collar, an outline head, then the colored shaft and head. An axis
        // that projects to a point is skipped instead of drawing garbage.
        assert!(triangles >= 8, "expected outlined screen geometry, got {triangles}");
        assert_eq!(
            triangles % 8,
            0,
            "outline and fill must stay paired per axis"
        );
    }

    #[test]
    fn scale_records_outlined_billboard_circles_without_internal_lines() {
        let mut frame = SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: 800,
            height: 600,
            texture_cache_budget_bytes: 0,
            stats: FrameStats::default(),
        };

        GizmoRenderSpec {
            mode: GizmoMode::Scale,
            active_axis: GizmoAxis::None,
            active_scale_sign: 0.0,
            origin: Vec3::ZERO,
            entity_scale: Vec3::ONE,
            entity_axes: [Vec3::X, Vec3::Y, Vec3::Z],
            enabled_scale_axes: [true; 3],
            presentation_scale: 1.0,
        }
        .append_to(&mut frame);

        assert!(frame
            .commands
            .commands()
            .iter()
            .all(|command| matches!(command, GraphicCommand::DrawScreenTriangleBatch { .. })));
        assert_eq!(
            frame
                .commands
                .commands()
                .iter()
                .find_map(|command| match command {
                    GraphicCommand::DrawScreenTriangleBatch { triangles } => Some(triangles.len()),
                    _ => None,
                }),
            // Six handles, each drawn twice: a dark outline disc and the
            // colored disc on top of it.
            Some(6 * 20 * 2 * 2)
        );
    }
}
