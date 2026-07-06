//! Transform operations: position, rotation, scale.
//! All values are in SI units (meters, radians).

use crate::errors::ScriptError;
use crate::host_api::ScriptContext;
use crate::node_handle::NodeHandle;
use crate::ScriptResult;

pub fn set_position(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    x: f32,
    y: f32,
    z: f32,
) -> ScriptResult<()> {
    handle.set_position(ctx, x, y, z)
}

pub fn set_rotation(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    x: f32,
    y: f32,
    z: f32,
) -> ScriptResult<()> {
    handle.set_rotation(ctx, x, y, z)
}

pub fn set_scale(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    x: f32,
    y: f32,
    z: f32,
) -> ScriptResult<()> {
    handle.set_scale(ctx, x, y, z)
}

pub fn get_position(ctx: &ScriptContext<'_>, handle: NodeHandle) -> ScriptResult<[f32; 3]> {
    handle.get_position(ctx)
}

pub fn move_by(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    dx: f32,
    dy: f32,
    dz: f32,
) -> ScriptResult<()> {
    handle.move_by(ctx, dx, dy, dz)
}

pub fn rotate_by(
    ctx: &mut ScriptContext<'_>,
    handle: NodeHandle,
    dx: f32,
    dy: f32,
    dz: f32,
) -> ScriptResult<()> {
    handle.rotate_by(ctx, dx, dy, dz)
}

/// Validate a handle is still alive. Returns InvalidHandle error if not.
pub fn ensure_valid(ctx: &ScriptContext<'_>, handle: NodeHandle) -> ScriptResult<()> {
    if handle.is_valid(ctx) {
        Ok(())
    } else {
        Err(ScriptError::InvalidHandle(handle.raw()))
    }
}
