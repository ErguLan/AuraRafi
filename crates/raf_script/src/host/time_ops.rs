//! Time operations: delta_time, elapsed_time.

use crate::host_api::ScriptContext;

pub fn get_delta_time(ctx: &ScriptContext<'_>) -> f32 {
    ctx.get_delta_time()
}

pub fn get_elapsed_time(ctx: &ScriptContext<'_>) -> f32 {
    ctx.get_elapsed_time()
}
