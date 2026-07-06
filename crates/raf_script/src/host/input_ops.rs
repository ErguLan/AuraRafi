//! Input operations: keyboard and mouse queries.

use crate::host_api::ScriptContext;

pub fn is_key_pressed(ctx: &ScriptContext<'_>, key: &str) -> bool {
    ctx.is_key_pressed(key)
}

pub fn was_key_just_pressed(ctx: &ScriptContext<'_>, key: &str) -> bool {
    ctx.was_key_just_pressed(key)
}

pub fn is_mouse_pressed(ctx: &ScriptContext<'_>, button: i32) -> bool {
    ctx.is_mouse_pressed(button)
}
