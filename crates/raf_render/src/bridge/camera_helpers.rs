//! Editor-only camera billboards and orientation guides on the shared AGB path.
//! The embedded SVG uses polylines only; compile its segments once, not per frame.
use std::sync::OnceLock;

use glam::{Mat4, Vec3};
use raf_core::runtime_config::GameCamera;
use raf_core::scene::{SceneGraph, SceneNodeId};

use crate::gizmo_visual::append_screen_line;
use crate::math::transform::project_point;
use crate::scene_renderer::SceneRenderFrame;

const SVG: &str = include_str!("../../assets/ui_icons/svg/camera.svg");
pub const CAMERA_ICON_SIZE: f32 = 32.0;
const OUTLINE: [u8; 4] = [12, 17, 23, 255];
const NORMAL: [u8; 4] = [235, 241, 248, 255];
const SELECTED: [u8; 4] = [255, 160, 40, 255];
type Segment = [[f32; 2]; 2];

#[derive(Default)]
pub struct CameraHelperCache {
    revision: Option<u64>,
    cameras: Vec<(SceneNodeId, Mat4, GameCamera)>,
}

fn svg_segments() -> &'static [Segment] {
    static SEGMENTS: OnceLock<Vec<Segment>> = OnceLock::new();
    SEGMENTS.get_or_init(|| {
        // A private, fixed embedded asset contract, not a general SVG importer.
        SVG.split("points=\"")
            .skip(1)
            .flat_map(|attribute| {
                let points = attribute.split('"').next().expect("embedded SVG points");
                let points: Vec<[f32; 2]> = points
                    .split_whitespace()
                    .map(|pair| {
                        let (x, y) = pair.split_once(',').expect("embedded SVG coordinate pair");
                        [
                            x.parse().expect("embedded SVG x"),
                            y.parse().expect("embedded SVG y"),
                        ]
                    })
                    .collect();
                points.windows(2).map(|p| [p[0], p[1]]).collect::<Vec<_>>()
            })
            .collect()
    })
}

fn center(world: Mat4, view_proj: &Mat4, size: [f32; 2]) -> Option<([f32; 2], f32)> {
    let (screen, depth) = project_point(world.col(3).truncate(), view_proj, size[0], size[1])?;
    // perspective_rh/orthographic_rh use WebGPU's 0..1 depth interval.
    (screen.iter().all(|v| v.is_finite()) && depth.is_finite() && (0.0..=1.0).contains(&depth))
        .then_some((screen, depth))
}

/// Billboard hit box matches the rendered glyph in logical viewport points.
/// Visible overlays win over meshes, including camera components on Parts.
pub fn pick_camera(
    scene: &SceneGraph,
    view_proj: &Mat4,
    click: [f32; 2],
    size: [f32; 2],
) -> Option<SceneNodeId> {
    scene
        .iter()
        .filter(|(id, node)| scene.is_valid_node(*id) && node.visible && node.game_camera.is_some())
        .filter_map(|(id, _)| {
            let (screen, depth) = center(scene.world_matrix(id), view_proj, size)?;
            let half = CAMERA_ICON_SIZE * 0.5 + 3.0;
            ((click[0] - screen[0]).abs() <= half && (click[1] - screen[1]).abs() <= half)
                .then_some((id, depth))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(id, _)| id)
}

/// Record camera decorations after scene geometry, before transform handles.
/// No camera geometry or overlays enter the player's scene command list.
impl CameraHelperCache {
    pub fn append_to(
        &mut self,
        scene: &SceneGraph,
        frame: &mut SceneRenderFrame,
        selected: &[SceneNodeId],
        pixel_scale: f32,
    ) {
        if self.revision != Some(scene.document_revision()) {
            self.cameras.clear();
            for (id, node) in scene.iter() {
                if scene.is_valid_node(id) && node.visible {
                    if let Some(lens) = &node.game_camera {
                        self.cameras
                            .push((id, scene.world_matrix(id), lens.clone()));
                    }
                }
            }
            self.revision = Some(scene.document_revision());
        }
        let size = [frame.width as f32, frame.height as f32];
        let pixel_scale = pixel_scale.clamp(0.1, 8.0);
        for (id, world, lens) in &self.cameras {
            let world = *world;
            let Some((screen, _)) = center(world, &frame.view_proj, size) else {
                continue;
            };
            let half = CAMERA_ICON_SIZE * 0.5 * pixel_scale;
            if screen[0] < -half
                || screen[0] > size[0] + half
                || screen[1] < -half
                || screen[1] > size[1] + half
            {
                continue;
            }
            let chosen = selected.contains(id);
            let color = if chosen { SELECTED } else { NORMAL };
            append_direction(frame, world, lens, chosen, color, pixel_scale);
            // Both passes use the same vector source. A dark rim remains legible
            // over the bright scene; the glyph always faces the editor camera.
            for (width, tint) in [(4.0, OUTLINE), (1.8, color)] {
                for segment in svg_segments() {
                    let point = |p: [f32; 2]| {
                        [
                            screen[0] + (p[0] - 12.0) / 24.0 * half * 2.0,
                            screen[1] + (p[1] - 12.0) / 24.0 * half * 2.0,
                        ]
                    };
                    append_screen_line(
                        &mut frame.commands,
                        point(segment[0]),
                        point(segment[1]),
                        width * pixel_scale,
                        tint,
                    );
                }
            }
        }
    }
}

fn append_direction(
    frame: &mut SceneRenderFrame,
    world: Mat4,
    lens: &GameCamera,
    selected: bool,
    color: [u8; 4],
    scale: f32,
) {
    let origin = world.col(3).truncate();
    let forward = world.transform_vector3(-Vec3::Z).normalize_or_zero();
    let up = world.transform_vector3(Vec3::Y).normalize_or_zero();
    let right = forward.cross(up).normalize_or_zero();
    let up = right.cross(forward).normalize_or_zero();
    if !forward.is_finite() || !up.is_finite() || right.length_squared() < 0.5 {
        return;
    }
    let tip = origin + forward * 1.5;
    let segments = [
        (origin, tip),
        (tip, tip - forward * 0.28 + right * 0.16),
        (tip, tip - forward * 0.28 - right * 0.16),
    ];
    for (width, tint) in [(4.0 * scale, OUTLINE), (1.8 * scale, color)] {
        for (a, b) in segments {
            frame.commands.draw_line(a, b, tint, width, true, -0.99);
        }
    }
    if !selected || lens.validate().is_err() {
        return;
    }
    // A short lens guide, not the camera's entire (possibly kilometer) frustum.
    // Its aspect follows this editor viewport; Play uses its own window aspect.
    let distance = lens.far.min(lens.near.max(1.5));
    let h = if lens.orthographic {
        lens.ortho_scale * 0.5
    } else {
        distance * (lens.fov_degrees.to_radians() * 0.5).tan()
    };
    let w = h * frame.width as f32 / frame.height.max(1) as f32;
    let plane = origin + forward * distance;
    let corners = [
        plane - right * w - up * h,
        plane + right * w - up * h,
        plane + right * w + up * h,
        plane - right * w + up * h,
    ];
    for index in 0..4 {
        let start = if lens.orthographic {
            corners[index] - forward * distance
        } else {
            origin
        };
        frame
            .commands
            .draw_line(start, corners[index], SELECTED, scale, true, -0.99);
        frame.commands.draw_line(
            corners[index],
            corners[(index + 1) % 4],
            SELECTED,
            scale,
            true,
            -0.99,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api_graphic_basic::command_list::{BasicCommandList, GraphicCommand};
    use crate::scene_renderer::FrameStats;

    #[test]
    fn camera_svg_billboard_is_pickable_and_direction_uses_the_parent_transform() {
        let mut scene = SceneGraph::new();
        let parent = scene.add_root("Pivot");
        let id = scene.add_child(parent, "Camera");
        scene.get_mut(parent).unwrap().rotation.y = 90.0;
        scene.get_mut(id).unwrap().game_camera = Some(GameCamera::default());
        scene.get_mut(id).unwrap().position = Vec3::new(-0.5, 0.0, 0.0);
        let world = scene.world_matrix(id);
        let view_proj = Mat4::IDENTITY;
        let screen = center(world, &view_proj, [800.0, 600.0]).unwrap().0;
        assert_eq!(
            pick_camera(&scene, &view_proj, screen, [800.0, 600.0]),
            Some(id)
        );
        let mut frame = SceneRenderFrame {
            commands: BasicCommandList::new(),
            view_proj,
            light_dir: Vec3::Y,
            width: 800,
            height: 600,
            texture_cache_budget_bytes: 0,
            stats: FrameStats::default(),
        };
        let mut cache = CameraHelperCache::default();
        cache.append_to(&scene, &mut frame, &[], 1.0);
        assert_eq!(svg_segments().len(), 11);
        assert!(frame
            .commands
            .commands()
            .iter()
            .any(|cmd| matches!(cmd, GraphicCommand::DrawScreenTriangleBatch { .. })));
        let direction = frame
            .commands
            .commands()
            .iter()
            .find_map(|cmd| match cmd {
                GraphicCommand::DrawLineBatch { lines, .. } => Some(lines[0].end - lines[0].start),
                _ => None,
            })
            .unwrap();
        assert!((direction.normalize() + Vec3::X).length() < 0.0001);
        scene.get_mut(id).unwrap().visible = false;
        assert_eq!(
            pick_camera(&scene, &view_proj, screen, [800.0, 600.0]),
            None
        );
        frame.commands = BasicCommandList::new();
        cache.append_to(&scene, &mut frame, &[], 1.0);
        assert!(frame.commands.commands().is_empty());
    }
}
