//! Thin Rhai adapter; validation and mutations belong to physics_api.
use super::rhai_backend::{with_ctx, RhaiResult};
use super::rhai_scene_api::{array, vector};
use crate::{NodeHandle, ScriptError};
use raf_core::scene::RigidBodyType;
use rhai::{Array, Engine, ImmutableString};

pub(super) fn register(engine: &mut Engine) {
    super::rhai_joint_api::register(engine);
    engine.register_fn(
        "configure_character",
        |h: &mut NodeHandle, radius: f64, height: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let defaults = raf_core::runtime_config::ProjectRuntimeSettings::default();
                let [jump, _, slope] = ctx.view.character_defaults.unwrap_or([
                    defaults.character_jump_height,
                    defaults.character_speed,
                    defaults.character_max_slope_degrees,
                ]);
                h.configure_character(
                    ctx,
                    radius as f32,
                    height as f32,
                    slope,
                    raf_core::scene::CharacterController::default().max_step_height,
                    jump,
                )
            })
        },
    );
    engine.register_fn(
        "character_move",
        |h: &mut NodeHandle, direction: Array| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let speed = ctx
                    .view
                    .character_defaults
                    .map(|values| values[1])
                    .unwrap_or_else(|| {
                        raf_core::runtime_config::ProjectRuntimeSettings::default().character_speed
                    });
                h.character_set_velocity(ctx, vector(direction)?.clamp_length_max(1.0) * speed)
            })
        },
    );
    engine.register_fn(
        "apply_force",
        |h: &mut NodeHandle, force: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.apply_force(ctx, vector(force)?))
        },
    );
    engine.register_fn(
        "apply_torque",
        |h: &mut NodeHandle, torque: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.apply_torque(ctx, vector(torque)?))
        },
    );
    engine.register_fn(
        "apply_impulse_at",
        |h: &mut NodeHandle, impulse: Array, point: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.apply_impulse_at(ctx, vector(impulse)?, vector(point)?))
        },
    );
    engine.register_fn(
        "configure_character",
        |h: &mut NodeHandle,
         radius: f64,
         height: f64,
         slope: f64,
         step: f64,
         jump: f64|
         -> RhaiResult<()> {
            with_ctx(|ctx| {
                h.configure_character(
                    ctx,
                    radius as f32,
                    height as f32,
                    slope as f32,
                    step as f32,
                    jump as f32,
                )
            })
        },
    );
    engine.register_fn(
        "character_set_velocity",
        |h: &mut NodeHandle, v: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.character_set_velocity(ctx, vector(v)?))
        },
    );
    engine.register_fn("character_jump", |h: &mut NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| h.character_jump(ctx))
    });
    engine.register_fn("character_grounded", |h: NodeHandle| -> RhaiResult<bool> {
        with_ctx(|ctx| h.character_grounded(ctx))
    });
    engine.register_fn("set_world_gravity", |v: Array| -> RhaiResult<()> {
        with_ctx(|ctx| ctx.set_world_gravity(vector(v)?))
    });
    engine.register_fn(
        "set_character_body",
        |h: &mut NodeHandle, enabled: bool| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_character_body(ctx, enabled))
        },
    );
    engine.register_fn("get_velocity", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| Ok(array(h.velocity(ctx)?.to_array())))
    });
    engine.register_fn(
        "set_velocity",
        |h: &mut NodeHandle, v: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_velocity(ctx, vector(v)?))
        },
    );
    engine.register_fn(
        "add_velocity",
        |h: &mut NodeHandle, v: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.add_velocity(ctx, vector(v)?))
        },
    );
    engine.register_fn(
        "apply_impulse",
        |h: &mut NodeHandle, v: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.apply_impulse(ctx, vector(v)?))
        },
    );
    engine.register_fn(
        "configure_body",
        |h: &mut NodeHandle,
         kind: ImmutableString,
         gravity: bool,
         trigger: bool,
         mass: f64,
         damping: f64|
         -> RhaiResult<()> {
            with_ctx(|ctx| {
                let kind = match kind.as_str() {
                    "static" => RigidBodyType::Static,
                    "dynamic" => RigidBodyType::Dynamic,
                    "kinematic" => RigidBodyType::Kinematic,
                    _ => {
                        return Err(ScriptError::InvalidArgument(
                            "Body type must be static, dynamic or kinematic".into(),
                        ))
                    }
                };
                h.configure_body(ctx, kind, gravity, trigger, mass as f32, damping as f32)
            })
        },
    );
    engine.register_fn(
        "set_physics_enabled",
        |h: &mut NodeHandle, enabled: bool| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_physics_enabled(ctx, enabled))
        },
    );
    engine.register_fn(
        "set_aabb_collider",
        |h: &mut NodeHandle, min: Array, max: Array| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_aabb_collider(ctx, vector(min)?, vector(max)?))
        },
    );
    engine.register_fn("remove_collider", |h: &mut NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| h.remove_collider(ctx))
    });
}
