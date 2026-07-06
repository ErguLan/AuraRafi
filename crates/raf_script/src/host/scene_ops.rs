//! Scene operations: get_node, spawn, destroy, find_child.

use crate::host_api::ScriptContext;
use crate::node_handle::NodeHandle;
use crate::ScriptResult;

/// Find an entity by name (searches the whole tree).
pub fn get_node(ctx: &ScriptContext<'_>, name: &str) -> Option<NodeHandle> {
    ctx.get_node(name)
}

/// Spawn a new entity with a primitive shape.
pub fn spawn_entity(
    ctx: &mut ScriptContext<'_>,
    name: &str,
    primitive: &str,
) -> ScriptResult<NodeHandle> {
    ctx.spawn_entity(name, primitive)
}

/// Destroy an entity by handle.
pub fn destroy_entity(ctx: &mut ScriptContext<'_>, handle: NodeHandle) -> ScriptResult<()> {
    ctx.destroy_entity(handle)
}
