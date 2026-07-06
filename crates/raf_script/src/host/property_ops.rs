//! Property operations: color, visibility, name, custom properties.

use crate::host_api::ScriptContext;
use crate::node_handle::NodeHandle;
use crate::value::ScriptValue;
use crate::ScriptResult;

pub fn set_color(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    r: u8,
    g: u8,
    b: u8,
    a: u8,
) -> ScriptResult<()> {
    handle.set_color(ctx, r, g, b, a)
}

pub fn set_visible(ctx: &mut ScriptContext<'_>, handle: NodeHandle, visible: bool) -> ScriptResult<()> {
    handle.set_visible(ctx, visible)
}

pub fn set_name(ctx: &mut ScriptContext<'_>, handle: NodeHandle, name: &str) -> ScriptResult<()> {
    handle.set_name(ctx, name)
}

pub fn get_property(
    ctx: &ScriptContext<'_>,
    handle: NodeHandle,
    key: &str,
) -> ScriptResult<ScriptValue> {
    handle.get_property(ctx, key)
}
