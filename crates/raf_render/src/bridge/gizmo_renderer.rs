//! ApiGraphicBasic recording for editor transform gizmos.
//!
//! Gizmos are renderer-owned authoring geometry, not retained UI widgets.
//! Recording them into the scene command list keeps CPU/GPU recovery and the
//! native viewport visually consistent without a second painter overlay.

use glam::Vec3;

use crate::api_graphic_basic::command_list::{BasicCommandList, BasicScreenTriangle};
use crate::gizmo::{GizmoAxis, GizmoMode};
use crate::gizmo_visual::{append_arrowhead, GizmoVisualProfile};
use crate::picking::{
    gizmo_scale_handle_radius, project_gizmo_arrow_scaled, project_gizmo_scale_handles,
    GIZMO_ARROWS, GIZMO_LENGTH, GIZMO_ROTATION_RADIUS,
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
        let length = GIZMO_LENGTH * self.presentation_scale.max(0.1);
        let profile = GizmoVisualProfile::translation();
        for (index, arrow) in GIZMO_ARROWS.iter().enumerate() {
            let axis = axis_for_index(index);
            let color = self.axis_color(axis, arrow.color);
            let width = if self.active_axis == axis {
                profile.shaft_width + 0.75
            } else {
                profile.shaft_width
            };
            let tip = self.origin + arrow.axis * length;
            frame
                .commands
                .draw_line(self.origin, tip, color, width, true, -0.99);

            // The shaft remains world-space, but the head is a filled
            // screen-facing billboard so perspective cannot turn it into the
            // old four-edge pyramid.
            if let Some(projected) = project_gizmo_arrow_scaled(
                self.origin,
                arrow,
                self.presentation_scale,
                &frame.view_proj,
                frame.width as f32,
                frame.height as f32,
            ) {
                append_arrowhead(&mut frame.commands, &projected, color, profile);
            }
        }
    }

    fn record_rotation(self, frame: &mut SceneRenderFrame) {
        let radius = GIZMO_ROTATION_RADIUS * self.presentation_scale.max(0.1);
        let planes = [(Vec3::Y, Vec3::Z), (Vec3::X, Vec3::Z), (Vec3::X, Vec3::Y)];
        for (axis_index, (axis_a, axis_b)) in planes.into_iter().enumerate() {
            let axis = axis_for_index(axis_index);
            let fallback = GIZMO_ARROWS[axis_index].color;
            let color = self.axis_color(axis, fallback);
            let width = self.axis_width(axis);
            let mut previous = self.origin + axis_a * radius;
            for step in 1..=48 {
                let angle = (step as f32 / 48.0) * std::f32::consts::TAU;
                let current =
                    self.origin + axis_a * (angle.cos() * radius) + axis_b * (angle.sin() * radius);
                frame
                    .commands
                    .draw_line(previous, current, color, width, true, -0.99);
                previous = current;
            }
        }
    }

    fn record_scale(self, frame: &mut SceneRenderFrame) {
        let handles = project_gizmo_scale_handles(
            self.origin,
            self.entity_scale,
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
            let radius =
                gizmo_scale_handle_radius(self.presentation_scale) + if active { 2.0 } else { 0.0 };
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

    fn axis_width(self, axis: GizmoAxis) -> f32 {
        if self.active_axis == axis {
            3.5
        } else {
            2.25
        }
    }
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
    fn translation_records_screen_billboard_heads_separately_from_shafts() {
        let mut frame = SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: 800,
            height: 600,
            stats: FrameStats::default(),
        };

        GizmoRenderSpec {
            mode: GizmoMode::Translate,
            active_axis: GizmoAxis::None,
            active_scale_sign: 0.0,
            origin: Vec3::ZERO,
            entity_scale: Vec3::ONE,
            presentation_scale: 1.0,
        }
        .append_to(&mut frame);

        assert!(frame.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawScreenTriangleBatch { triangles } if !triangles.is_empty()
        )));
        assert!(frame.commands.commands().iter().any(|command| matches!(
            command,
            GraphicCommand::DrawLineBatch { lines, no_depth_test: true } if !lines.is_empty()
        )));
    }

    #[test]
    fn scale_records_six_external_billboard_circles_without_internal_lines() {
        let mut frame = SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj: Mat4::IDENTITY,
            light_dir: Vec3::Y,
            width: 800,
            height: 600,
            stats: FrameStats::default(),
        };

        GizmoRenderSpec {
            mode: GizmoMode::Scale,
            active_axis: GizmoAxis::None,
            active_scale_sign: 0.0,
            origin: Vec3::ZERO,
            entity_scale: Vec3::ONE,
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
            Some(6 * 20 * 2)
        );
    }
}
