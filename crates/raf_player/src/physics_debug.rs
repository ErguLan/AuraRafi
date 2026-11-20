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
const BONE: [u8; 4] = [255, 48, 64, 255];
const VELOCITY: [u8; 4] = [80, 190, 255, 255];
const WEIGHT: [u8; 4] = [255, 228, 84, 255];
const LOAD: [u8; 4] = [160, 124, 255, 255];
const ANGULAR: [u8; 4] = [255, 124, 195, 255];
const MAX_BONES: usize = 2048;
const MAX_IMPACTS: usize = 512;

pub(crate) fn stats(world: &RuntimeWorld) -> String {
    let settings = world.project_settings();
    let status = world.status();
    let mut result = format!("#{} · {:.2} s", status.fixed_ticks, status.elapsed_seconds);
    if settings.runtime.animation_debug_draw {
        result.push_str(&format!(
            " · {} / {}",
            world.animation_bones().len().min(MAX_BONES),
            world.animation_bones().len()
        ));
    }
    if settings.enable_physics && settings.runtime.physics_debug_draw {
        let advanced = settings.runtime.advanced_physics;
        let body = world.scene.iter().map(|(_, node)| node).find(|node| {
            !node.name.is_empty()
                && node.rigid_body.enabled
                && node.rigid_body.body_type == RigidBodyType::Dynamic
                && (advanced || node.rigid_body.is_character)
        });
        if let Some(node) = body {
            let body = &node.rigid_body;
            let weight = weight(world, body);
            let force = world
                .physics_debug_loads()
                .iter()
                .find(|load| load.body == node.uuid)
                .map_or(0.0, |load| load.force.length());
            let name = node.name.chars().take(24).collect::<String>();
            result.push_str(&format!(
                " · {name} · {:.2} kg · {:.2} m/s · W {:.1} N · F {:.1} N",
                body.mass,
                body.velocity.length(),
                weight.length(),
                force
            ));
        }
        let impulse = world
            .physics_impacts()
            .iter()
            .map(|hit| hit.normal_impulse)
            .fold(0.0_f32, f32::max);
        if impulse > 0.0 {
            result.push_str(&format!(" · J {impulse:.2} N·s"));
        }
    }
    result
}

fn weight(world: &RuntimeWorld, body: &raf_core::scene::RigidBody) -> Vec3 {
    if !body.use_gravity {
        return Vec3::ZERO;
    }
    let scale = if world.project_settings().runtime.advanced_physics {
        body.gravity_scale
    } else {
        1.0
    };
    world.physics_gravity() * (body.mass * scale)
}

pub(crate) fn append(commands: &mut BasicCommandList, world: &RuntimeWorld) {
    let project = world.project_settings();
    let physics = project.enable_physics && project.runtime.physics_debug_draw;
    let animation = project.runtime.animation_debug_draw;
    if !physics && !animation {
        return;
    }

    let scene = &world.scene;
    let transforms = WorldTransformCache::build(scene);
    if animation {
        commands.draw_line_batch(
            world
                .animation_bones()
                .iter()
                .take(MAX_BONES)
                .filter_map(|&(id, identity)| {
                    let node = scene
                        .get(id)
                        .filter(|node| node.uuid == identity && !node.name.is_empty())?;
                    let joint = transforms.world_matrix(id)?.w_axis.truncate();
                    let parent = transforms.world_matrix(node.parent?)?.w_axis.truncate();
                    (joint.is_finite() && parent.is_finite()).then(|| line(parent, joint, BONE))
                }),
            true,
        );
    }
    if !physics {
        return;
    }
    let up = (-world.physics_gravity())
        .try_normalize()
        .unwrap_or(Vec3::Y);
    let mut lines = Vec::new();
    let loads: std::collections::HashMap<_, _> = world
        .physics_debug_loads()
        .iter()
        .map(|load| (load.body, load))
        .collect();
    for (id, node) in scene
        .iter()
        .filter(|(id, node)| {
            scene.is_valid_node(*id)
                && (node.character_controller.is_some()
                    || node.collider.collider_type == ColliderType::Aabb
                    || (node.rigid_body.enabled
                        && node.rigid_body.body_type != RigidBodyType::Static))
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
        } else if node.collider.collider_type == ColliderType::Aabb {
            lines.extend(node.collider.aabb.edges().into_iter().map(|[a, b]| {
                line(
                    transform.transform_point3(a + node.collider.offset),
                    transform.transform_point3(b + node.collider.offset),
                    color,
                )
            }));
        }
        let body = &node.rigid_body;
        let origin = transform.w_axis.truncate();
        let advanced = project.runtime.advanced_physics;
        if body.enabled
            && (body.body_type == RigidBodyType::Kinematic
                || (body.body_type == RigidBodyType::Dynamic && (advanced || body.is_character)))
        {
            arrow(&mut lines, origin, body.velocity, 0.18, VELOCITY);
            if advanced {
                arrow(&mut lines, origin, body.angular_velocity, 0.12, ANGULAR);
            }
            if body.body_type == RigidBodyType::Dynamic {
                arrow(&mut lines, origin, weight(world, body), 0.055, WEIGHT);
            }
        }
        if let Some(load) = loads.get(&node.uuid) {
            arrow(&mut lines, origin, load.force, 0.055, LOAD);
            arrow(&mut lines, origin, load.torque, 0.12, ANGULAR);
            arrow(&mut lines, origin, load.impulse, 0.32, MOVING);
        }
    }
    for impact in world.physics_impacts().iter().take(MAX_IMPACTS) {
        arrow(
            &mut lines,
            impact.point,
            impact.normal * impact.normal_impulse,
            0.32,
            MOVING,
        );
    }
    commands.draw_line_batch(lines, true);
}

fn arrow(lines: &mut Vec<BasicLine>, origin: Vec3, vector: Vec3, scale: f32, color: [u8; 4]) {
    if !origin.is_finite() || !vector.is_finite() {
        return;
    }
    let Some(direction) = vector.try_normalize() else {
        return;
    };
    let length = (scale * vector.length()).clamp(0.05, 3.0);
    let tip = origin + direction * length;
    let head = length * 0.25;
    let tail = tip - direction * head;
    let right = direction.any_orthonormal_vector();
    let up = direction.cross(right);
    lines.push(line(origin, tip, color));
    for side in [right, -right, up, -up] {
        lines.push(line(tip, tail + side * head * 0.45, color));
    }
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
