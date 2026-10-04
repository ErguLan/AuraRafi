//! Rhai registration only; domain operations live in the shared Host API.
use super::rhai_backend::{with_ctx, RhaiResult};
use crate::{NodeHandle, ScriptError, ScriptResult};
use glam::{EulerRot, Quat, Vec3};
use rhai::{Array, Dynamic, Engine, ImmutableString, Map};

fn vector(values: Array) -> ScriptResult<Vec3> {
    if values.len() != 3 {
        return Err(ScriptError::InvalidArgument(
            "Vector requires three finite numbers".into(),
        ));
    }
    let mut out = [0.0; 3];
    for (i, value) in values.into_iter().enumerate() {
        out[i] = if value.is::<f64>() {
            value.cast::<f64>() as f32
        } else if value.is::<rhai::INT>() {
            value.cast::<rhai::INT>() as f32
        } else {
            f32::NAN
        };
    }
    let value = Vec3::from_array(out);
    if !value.is_finite() {
        return Err(ScriptError::InvalidArgument(
            "Vector requires three finite numbers".into(),
        ));
    }
    Ok(value)
}
fn array(values: [f32; 3]) -> Array {
    values
        .into_iter()
        .map(|v| Dynamic::from_float(v as f64))
        .collect()
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn(
        "entity",
        |reference: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| Ok(ctx.entity(&reference).unwrap_or_else(NodeHandle::invalid)))
        },
    );
    engine.register_fn("entity", |handle: NodeHandle| -> NodeHandle { handle });
    engine.register_fn(
        "get_node_by_uuid",
        |reference: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| Ok(ctx.entity(&reference).unwrap_or_else(NodeHandle::invalid)))
        },
    );
    engine.register_fn(
        "get_node_by_path",
        |reference: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| Ok(ctx.entity(&reference).unwrap_or_else(NodeHandle::invalid)))
        },
    );
    engine.register_fn("node_uuid", |handle: NodeHandle| -> RhaiResult<String> {
        with_ctx(|ctx| {
            Ok(ctx
                .scene
                .get(handle.resolve(ctx)?)
                .ok_or(ScriptError::InvalidHandle(handle.raw()))?
                .uuid
                .to_string())
        })
    });
    engine.register_fn("get_active_camera", || -> RhaiResult<NodeHandle> {
        with_ctx(|ctx| Ok(ctx.active_camera().unwrap_or_else(NodeHandle::invalid)))
    });
    engine.register_fn("activate_camera", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| ctx.activate_camera(handle))
    });
    engine.register_fn("clear_active_camera", || -> RhaiResult<()> {
        with_ctx(|ctx| ctx.clear_camera())
    });
    engine.register_fn("is_client", || -> RhaiResult<bool> {
        with_ctx(|ctx| Ok(ctx.view.role != crate::view::ScriptRole::Server))
    });
    engine.register_fn("runtime_role", || -> RhaiResult<String> {
        with_ctx(|ctx| {
            Ok(match ctx.view.role {
                crate::view::ScriptRole::Local => "local",
                crate::view::ScriptRole::Client => "client",
                crate::view::ScriptRole::Server => "server",
            }
            .into())
        })
    });
    engine.register_fn(
        "spawn_camera",
        |name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                let handle = ctx.spawn_entity(&name, "empty")?;
                handle.add_camera(ctx)?;
                Ok(handle)
            })
        },
    );
    engine.register_fn("add_camera", |h: &mut NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| h.add_camera(ctx))
    });
    engine.register_fn("remove_camera", |h: &mut NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| h.remove_camera(ctx))
    });
    engine.register_fn("has_camera", |h: NodeHandle| -> RhaiResult<bool> {
        with_ctx(|ctx| {
            Ok(ctx
                .scene
                .get(h.resolve(ctx)?)
                .is_some_and(|n| n.game_camera.is_some()))
        })
    });
    engine.register_fn(
        "set_fov",
        |h: &mut NodeHandle, fov: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let mut lens = h.camera_lens(ctx)?;
                lens.fov_degrees = fov as f32;
                h.set_camera_lens(ctx, lens)
            })
        },
    );
    engine.register_fn(
        "set_clip",
        |h: &mut NodeHandle, near: f64, far: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let mut lens = h.camera_lens(ctx)?;
                lens.near = near as f32;
                lens.far = far as f32;
                h.set_camera_lens(ctx, lens)
            })
        },
    );
    engine.register_fn(
        "set_orthographic",
        |h: &mut NodeHandle, orthographic: bool, scale: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let mut lens = h.camera_lens(ctx)?;
                lens.orthographic = orthographic;
                lens.ortho_scale = scale as f32;
                h.set_camera_lens(ctx, lens)
            })
        },
    );
    engine.register_fn("get_camera_lens", |h: NodeHandle| -> RhaiResult<Map> {
        with_ctx(|ctx| {
            let lens = h.camera_lens(ctx)?;
            let mut map = Map::new();
            map.insert(
                "fov_degrees".into(),
                Dynamic::from_float(lens.fov_degrees as f64),
            );
            map.insert("near".into(), Dynamic::from_float(lens.near as f64));
            map.insert("far".into(), Dynamic::from_float(lens.far as f64));
            map.insert("orthographic".into(), lens.orthographic.into());
            map.insert(
                "ortho_scale".into(),
                Dynamic::from_float(lens.ortho_scale as f64),
            );
            Ok(map)
        })
    });
    engine.register_fn(
        "look_at",
        |h: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| h.look_at(ctx, Vec3::new(x as f32, y as f32, z as f32), Vec3::Y))
        },
    );
    engine.register_fn(
        "look_at",
        |h: &mut NodeHandle, target: NodeHandle, offset: Array| -> RhaiResult<()> {
            with_ctx(|ctx| {
                let aim = Vec3::from_array(target.get_position(ctx)?) + vector(offset)?;
                h.look_at(ctx, aim, Vec3::Y)
            })
        },
    );
    engine.register_fn(
        "follow",
        |h: &mut NodeHandle,
         target: NodeHandle,
         offset: Array,
         aim: Array,
         sharpness: f64,
         dt: f64|
         -> RhaiResult<()> {
            with_ctx(|ctx| {
                if !sharpness.is_finite()
                    || sharpness < 0.0
                    || !dt.is_finite()
                    || dt < 0.0
                    || dt > 1.0
                {
                    return Err(ScriptError::InvalidArgument(
                        "Follow requires sharpness >= 0 and dt in 0..1".into(),
                    ));
                }
                let point = Vec3::from_array(target.get_position(ctx)?);
                let desired = point + vector(offset)?;
                let aim = point + vector(aim)?;
                let current = Vec3::from_array(h.get_position(ctx)?);
                let alpha = if sharpness == 0.0 {
                    1.0
                } else {
                    (1.0 - (-sharpness * dt).exp()) as f32
                };
                let position = current.lerp(desired, alpha);
                // Validate orientation before changing position; no half-applied pose.
                if (aim - position).length_squared() < 1e-8
                    || (aim - position).cross(Vec3::Y).length_squared() < 1e-8
                {
                    return Err(ScriptError::InvalidArgument("Invalid follow aim".into()));
                }
                h.look_at(ctx, aim - (position - current), Vec3::Y)?;
                h.set_position(ctx, position.x, position.y, position.z)
            })
        },
    );
    engine.register_fn("get_local_position", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| {
            Ok(array(
                ctx.scene
                    .get(h.resolve(ctx)?)
                    .ok_or(ScriptError::InvalidHandle(h.raw()))?
                    .position
                    .to_array(),
            ))
        })
    });
    engine.register_fn("get_world_position", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| Ok(array(h.get_position(ctx)?)))
    });
    engine.register_fn(
        "set_world_position",
        |h: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_position(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn("get_local_rotation", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| Ok(array(h.get_rotation(ctx)?)))
    });
    engine.register_fn(
        "set_local_rotation",
        |h: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_rotation(ctx, x as f32, y as f32, z as f32))
        },
    );
    engine.register_fn("get_world_rotation", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| {
            let (_, q, _) = ctx
                .scene
                .world_matrix(h.resolve(ctx)?)
                .to_scale_rotation_translation();
            let (y, x, z) = q.to_euler(EulerRot::YXZ);
            Ok(array([x, y, z]))
        })
    });
    engine.register_fn(
        "set_local_position",
        |h: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_local_position(ctx, Vec3::new(x as f32, y as f32, z as f32)))
        },
    );
    engine.register_fn(
        "set_world_rotation",
        |h: &mut NodeHandle, x: f64, y: f64, z: f64| -> RhaiResult<()> {
            with_ctx(|ctx| {
                h.set_world_rotation(
                    ctx,
                    Quat::from_euler(EulerRot::YXZ, y as f32, x as f32, z as f32),
                )
            })
        },
    );
    engine.register_fn("get_children", |h: NodeHandle| -> RhaiResult<Array> {
        with_ctx(|ctx| {
            let id = h.resolve(ctx)?;
            let children = &ctx
                .scene
                .get(id)
                .ok_or(ScriptError::InvalidHandle(h.raw()))?
                .children;
            if children.len() > 1024 {
                return Err(ScriptError::InvalidArgument(
                    "Child query exceeds 1024 entries".into(),
                ));
            }
            Ok(children
                .iter()
                .filter(|id| ctx.scene.is_valid_node(**id))
                .map(|id| Dynamic::from(NodeHandle::scoped(*id, ctx)))
                .collect())
        })
    });
    engine.register_fn(
        "set_parent",
        |h: &mut NodeHandle, parent: NodeHandle, keep_world: bool| -> RhaiResult<()> {
            with_ctx(|ctx| h.reparent(ctx, Some(parent), keep_world))
        },
    );
    engine.register_fn(
        "detach_parent",
        |h: &mut NodeHandle, keep_world: bool| -> RhaiResult<()> {
            with_ctx(|ctx| h.reparent(ctx, None, keep_world))
        },
    );
}
