//! Rhai registration for the particle-system node API.
//!
//! Global aliases (`particle_play`) match the flat style used by the rest of
//! the scripting API. The `particles` static module exposes the shorter
//! `particles::play(handle)` form requested by gameplay scripts.

use super::rhai_backend::{with_ctx, RhaiResult};
use crate::NodeHandle;
use rhai::{Engine, Module};

fn emit_count(count: i64) -> RhaiResult<u32> {
    if !(0..=100_000).contains(&count) {
        return Err("particle emit count must be between 0 and 100000".into());
    }
    Ok(count as u32)
}

fn finite_scale(scale: f64) -> RhaiResult<f32> {
    if !scale.is_finite() {
        return Err("particle scale must be finite".into());
    }
    Ok((scale as f32).clamp(0.0, 8.0))
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("particle_play", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_play(ctx))
    });
    engine.register_fn("particle_pause", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_pause(ctx))
    });
    engine.register_fn("particle_stop", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_stop(ctx))
    });
    engine.register_fn("particle_freeze", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_freeze(ctx))
    });
    engine.register_fn(
        "particle_emit",
        |handle: NodeHandle, count: i64| -> RhaiResult<()> {
            let count = emit_count(count)?;
            with_ctx(|ctx| handle.particle_emit(ctx, count))
        },
    );
    engine.register_fn(
        "particle_set_rate",
        |handle: NodeHandle, scale: f64| -> RhaiResult<()> {
            let scale = finite_scale(scale)?;
            with_ctx(|ctx| handle.particle_set_rate(ctx, scale))
        },
    );
    engine.register_fn(
        "particle_set_time_scale",
        |handle: NodeHandle, scale: f64| -> RhaiResult<()> {
            let scale = finite_scale(scale)?;
            with_ctx(|ctx| handle.particle_set_time_scale(ctx, scale))
        },
    );

    let mut module = Module::new();
    module.set_native_fn("play", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_play(ctx))
    });
    module.set_native_fn("pause", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_pause(ctx))
    });
    module.set_native_fn("stop", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_stop(ctx))
    });
    module.set_native_fn("freeze", |handle: NodeHandle| -> RhaiResult<()> {
        with_ctx(|ctx| handle.particle_freeze(ctx))
    });
    module.set_native_fn("emit", |handle: NodeHandle, count: i64| -> RhaiResult<()> {
        let count = emit_count(count)?;
        with_ctx(|ctx| handle.particle_emit(ctx, count))
    });
    module.set_native_fn(
        "set_rate",
        |handle: NodeHandle, scale: f64| -> RhaiResult<()> {
            let scale = finite_scale(scale)?;
            with_ctx(|ctx| handle.particle_set_rate(ctx, scale))
        },
    );
    module.set_native_fn(
        "set_time_scale",
        |handle: NodeHandle, scale: f64| -> RhaiResult<()> {
            let scale = finite_scale(scale)?;
            with_ctx(|ctx| handle.particle_set_time_scale(ctx, scale))
        },
    );
    engine.register_static_module("particles", module.into());
}
