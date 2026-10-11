//! Thin Rhai adapter; validation and mutations belong to physics_api.
use super::rhai_backend::{with_ctx, RhaiResult};
use super::rhai_scene_api::{array, vector};
use crate::{NodeHandle, ScriptError};
use raf_core::scene::RigidBodyType;
use rhai::{Array, Engine, ImmutableString};

pub(super) fn register(engine: &mut Engine) {
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
