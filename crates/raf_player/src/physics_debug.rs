//! Debug geometry for the live physics snapshot through ApiGraphicBasic.
use glam::Vec3;
use raf_core::scene::{ColliderType, RigidBodyType, WorldTransformCache};
use raf_render::api_graphic_basic::command_list::{BasicCommandList, BasicLine};
use raf_runtime::RuntimeWorld;

const MAX_COLLIDERS: usize = 512;
const CIRCLE_STEPS: usize = 16;
const STATIC: [u8; 4] = [72, 204, 180, 255];
const MOVING: [u8; 4] = [252, 174, 72, 255];
const SENSOR: [u8; 4] = [211, 127, 251, 255];
const SUPPORT: [u8; 4] = [105, 232, 101, 255];

pub(crate) fn append(commands: &mut BasicCommandList, world: &RuntimeWorld) {
    let project = world.project_settings();
    if !project.enable_physics || !project.runtime.physics_debug_draw {
        return;
    }

    let scene = &world.scene;
    let transforms = WorldTransformCache::build(scene);
    let up = (-world.physics_gravity())
        .try_normalize()
        .unwrap_or(Vec3::Y);
    let mut lines = Vec::new();
    for (id, node) in scene
        .iter()
        .filter(|(id, node)| {
            scene.is_valid_node(*id)
                && (node.character_controller.is_some()
                    || node.collider.collider_type == ColliderType::Aabb)
        })
        .take(MAX_COLLIDERS)
    {
        let Some(transform) = transforms.world_matrix(id) else {
            continue;
        };
        let color = if node.rigid_body.enabled && node.rigid_body.is_trigger {
            SENSOR
        } else if node.rigid_body.enabled && node.rigid_body.body_type != RigidBodyType::Static {
            MOVING
        } else {
            STATIC
        };
        if let Some(character) = node
            .character_controller
            .as_ref()
            .filter(|_| node.rigid_body.enabled && node.rigid_body.is_character)
        {
            let center = transform.w_axis.truncate();
            capsule(
                &mut lines,
                center,
                up,
                character.radius,
                character.height,
                color,
            );
            if character.state.grounded {
                let foot = center - up * (character.height * 0.5);
                lines.push(line(
                    foot,
                    foot + character.state.ground_normal * 0.6,
                    SUPPORT,
                ));
            }
        } else {
            lines.extend(node.collider.aabb.edges().into_iter().map(|[a, b]| {
                line(
                    transform.transform_point3(a + node.collider.offset),
                    transform.transform_point3(b + node.collider.offset),
                    color,
                )
            }));
        }
    }
    lines.extend(
        world
            .physics_impacts()
            .iter()
            .map(|impact| line(impact.point, impact.point + impact.normal * 0.35, MOVING)),
    );
    commands.draw_line_batch(lines, true);
}

fn capsule(
    lines: &mut Vec<BasicLine>,
    center: Vec3,
    up: Vec3,
    radius: f32,
    height: f32,
    color: [u8; 4],
) {
    let right = up.any_orthonormal_vector();
    let forward = up.cross(right);
    let half_segment = (height * 0.5 - radius).max(0.0);
    let top = center + up * half_segment;
    let bottom = center - up * half_segment;
    for i in 0..CIRCLE_STEPS {
        let angle = |i| i as f32 * std::f32::consts::TAU / CIRCLE_STEPS as f32;
        let radial = |i| (right * angle(i).cos() + forward * angle(i).sin()) * radius;
        lines.push(line(top + radial(i), top + radial(i + 1), color));
        lines.push(line(bottom + radial(i), bottom + radial(i + 1), color));
    }
    for side in [right, forward, -right, -forward] {
        lines.push(line(bottom + side * radius, top + side * radius, color));
        for i in 0..CIRCLE_STEPS / 4 {
            let arc = |i| {
                let angle = i as f32 * std::f32::consts::TAU / CIRCLE_STEPS as f32;
                (side * angle.cos(), up * angle.sin())
            };
            let (a, ya) = arc(i);
            let (b, yb) = arc(i + 1);
            lines.push(line(
                top + (a + ya) * radius,
                top + (b + yb) * radius,
                color,
            ));
            lines.push(line(
                bottom + (a - ya) * radius,
                bottom + (b - yb) * radius,
                color,
            ));
        }
    }
}

fn line(start: Vec3, end: Vec3, color: [u8; 4]) -> BasicLine {
    BasicLine {
        start,
        end,
        color,
        width: 1.25,
        depth_bias: 0.0,
    }
}
