use super::rhai_backend::{with_ctx, RhaiResult};
use super::rhai_scene_api::vector;
use crate::NodeHandle;
use raf_core::scene::joint::{JointKind, JointMotor};
use rhai::{Array, Engine, ImmutableString};

fn invalid(message: &str) -> Box<rhai::EvalAltResult> {
    message.to_owned().into()
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn(
        "attach_collider_to_parent",
        |owner: &mut NodeHandle, attached: bool| -> RhaiResult<()> {
            with_ctx(|ctx| owner.attach_collider_to_parent(ctx, attached))
        },
    );
    engine.register_fn(
        "set_collision_filter",
        |owner: &mut NodeHandle, layer: i64, mask: i64| -> RhaiResult<()> {
            let layer =
                u32::try_from(layer).map_err(|_| invalid("Invalid collision layer bits"))?;
            let mask = u32::try_from(mask).map_err(|_| invalid("Invalid collision mask bits"))?;
            with_ctx(|ctx| owner.set_collision_filter(ctx, layer, mask))
        },
    );
    engine.register_fn(
        "add_physics_joint",
        |owner: &mut NodeHandle,
         target: NodeHandle,
         kind: ImmutableString,
         a: Array,
         b: Array,
         axis_a: Array,
         axis_b: Array|
         -> RhaiResult<i64> {
            let kind = match kind.as_str() {
                "fixed" => JointKind::Fixed,
                "hinge" => JointKind::Hinge,
                "slider" => JointKind::Slider,
                "ball" => JointKind::Ball,
                "spring" => JointKind::Spring,
                _ => return Err(invalid("Unknown physical joint kind")),
            };
            with_ctx(|ctx| {
                Ok(owner.add_physics_joint(
                    ctx,
                    target,
                    kind,
                    [vector(a)?, vector(b)?],
                    [vector(axis_a)?, vector(axis_b)?],
                )? as i64)
            })
        },
    );
    engine.register_fn(
        "configure_joint_motion",
        |owner: &mut NodeHandle,
         index: i64,
         min: f64,
         max: f64,
         velocity: f64,
         max_force: f64,
         break_impulse: f64|
         -> RhaiResult<()> {
            let index = usize::try_from(index).map_err(|_| invalid("Invalid joint index"))?;
            if !max_force.is_finite()
                || max_force < 0.0
                || !break_impulse.is_finite()
                || break_impulse < 0.0
            {
                return Err(invalid(
                    "Joint motor force and break threshold must be finite and nonnegative",
                ));
            }
            with_ctx(|ctx| {
                owner.configure_joint_motion(
                    ctx,
                    index,
                    Some([min as f32, max as f32]),
                    (max_force > 0.0).then_some(JointMotor {
                        velocity: velocity as f32,
                        max_force: max_force as f32,
                    }),
                    (break_impulse > 0.0).then_some(break_impulse as f32),
                )
            })
        },
    );
    engine.register_fn(
        "configure_joint_spring",
        |owner: &mut NodeHandle,
         index: i64,
         rest_length: f64,
         stiffness: f64,
         damping: f64|
         -> RhaiResult<()> {
            let index = usize::try_from(index).map_err(|_| invalid("Invalid joint index"))?;
            with_ctx(|ctx| {
                owner.configure_joint_spring(
                    ctx,
                    index,
                    rest_length as f32,
                    stiffness as f32,
                    damping as f32,
                )
            })
        },
    );
}
