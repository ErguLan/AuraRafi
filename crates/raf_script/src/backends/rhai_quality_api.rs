//! Rhai syntax only; all validation and local/effective rules live in the Host API.
use super::rhai_backend::{from_dynamic, to_dynamic, with_ctx, RhaiResult};
use rhai::{Dynamic, Engine, ImmutableString};

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("get_quality", || -> RhaiResult<Dynamic> {
        with_ctx(|ctx| ctx.get_quality().map(to_dynamic))
    });
    engine.register_fn(
        "set_quality",
        |path: ImmutableString, value: Dynamic| -> RhaiResult<()> {
            with_ctx(|ctx| ctx.set_quality(&path, from_dynamic(value)?))
        },
    );
    engine.register_fn("clear_quality", || -> RhaiResult<()> {
        with_ctx(|ctx| ctx.clear_quality())
    });
}
