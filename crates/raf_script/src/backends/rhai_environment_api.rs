//! Rhai syntax only; all validation and scope rules live in the shared Host API.
use super::rhai_backend::{from_dynamic, to_dynamic, with_ctx, RhaiResult};
use rhai::{Dynamic, Engine, ImmutableString};
pub(super) fn register(engine: &mut Engine) {
    engine.register_fn(
        "get_environment",
        |path: ImmutableString| -> RhaiResult<Dynamic> {
            with_ctx(|ctx| ctx.get_environment(&path).map(to_dynamic))
        },
    );
    engine.register_fn(
        "set_environment",
        |path: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.set_environment(&path, from_dynamic(value)?))
        },
    );
    engine.register_fn(
        "get_view_environment",
        |path: ImmutableString| -> RhaiResult<Dynamic> {
            with_ctx(|ctx| ctx.get_view_environment(&path).map(to_dynamic))
        },
    );
    engine.register_fn(
        "set_view_environment",
        |path: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.set_view_environment(&path, from_dynamic(value)?))
        },
    );
    engine.register_fn("clear_view_environment", || -> RhaiResult<()> {
        with_ctx(|ctx| ctx.clear_view_environment(None))
    });
    engine.register_fn(
        "clear_view_environment",
        |path: ImmutableString| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.clear_view_environment(Some(&path)))
        },
    );
}
