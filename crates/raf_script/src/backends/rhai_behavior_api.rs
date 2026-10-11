//! Rhai syntax adapters for the language-independent behavior/reference API.
use super::rhai_backend::{with_ctx, RhaiResult};
use super::rhai_scene_api::vector;
use crate::NodeHandle;
use rhai::{Array, Dynamic, Engine, ImmutableString, Scope};

#[derive(Clone, Copy)]
struct ScriptAccess;
#[derive(Clone, Copy)]
struct WorldAccess;

pub(super) fn seed_scope(scope: &mut Scope<'static>) {
    // Compatibility only: new public examples use owner() and entity_parameter().
    scope.push_constant("script", ScriptAccess);
    scope.push_constant("game", WorldAccess);
}

pub(super) fn register(engine: &mut Engine) {
    engine.register_fn("owner", || -> RhaiResult<NodeHandle> {
        with_ctx(|ctx| Ok(ctx.owner().unwrap_or_else(NodeHandle::invalid)))
    });
    engine.register_fn(
        "entity_parameter",
        |name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(ctx
                    .entity_parameter(&name)?
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_fn(
        "get_entity_reference",
        |handle: NodeHandle, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(handle
                    .entity_reference(ctx, &name)?
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_type_with_name::<ScriptAccess>("Script");
    engine.register_type_with_name::<WorldAccess>("World");
    engine.register_get("parent", |_: &mut ScriptAccess| -> RhaiResult<NodeHandle> {
        with_ctx(|ctx| Ok(ctx.self_node().unwrap_or_else(NodeHandle::invalid)))
    });
    engine.register_get("parent", |h: &mut NodeHandle| -> RhaiResult<NodeHandle> {
        with_ctx(|ctx| Ok(ctx.get_parent(*h)?.unwrap_or_else(NodeHandle::invalid)))
    });
    engine.register_fn(
        "child",
        |h: NodeHandle, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(ctx
                    .find_child(h, &name)?
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_indexer_get(
        |h: &mut NodeHandle, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(ctx
                    .find_child(*h, &name)?
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_fn(
        "child",
        |_: WorldAccess, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(crate::references::root_child(ctx.scene, &name)
                    .map(|id| NodeHandle::scoped(id, ctx))
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_indexer_get(
        |_: &mut WorldAccess, name: ImmutableString| -> RhaiResult<NodeHandle> {
            with_ctx(|ctx| {
                Ok(crate::references::root_child(ctx.scene, &name)
                    .map(|id| NodeHandle::scoped(id, ctx))
                    .unwrap_or_else(NodeHandle::invalid))
            })
        },
    );
    engine.register_fn(
        "parameter",
        |name: ImmutableString, default: Dynamic| -> RhaiResult<Dynamic> {
            with_ctx(|ctx| {
                ctx.parameter(&name, super::rhai_backend::from_dynamic(default)?)
                    .map(super::rhai_backend::to_dynamic)
            })
        },
    );
    engine.register_fn(
        "set_capability",
        |h: &mut NodeHandle, name: ImmutableString, enabled: bool| -> RhaiResult<()> {
            with_ctx(|ctx| h.set_capability(ctx, &name, enabled))
        },
    );
    engine.register_fn(
        "has_capability",
        |h: NodeHandle, name: ImmutableString| -> RhaiResult<bool> {
            with_ctx(|ctx| h.has_capability(ctx, &name))
        },
    );
    engine.register_fn(
        "query_capability",
        |name: ImmutableString, limit: rhai::INT| -> RhaiResult<Array> {
            with_ctx(|ctx| {
                Ok(ctx
                    .query_capability(&name, None, usize::try_from(limit).unwrap_or(0))?
                    .into_iter()
                    .map(Dynamic::from)
                    .collect())
            })
        },
    );
    engine.register_fn(
        "query_nearby",
        |name: ImmutableString, point: Array, radius: f64, limit: rhai::INT| -> RhaiResult<Array> {
            with_ctx(|ctx| {
                Ok(ctx
                    .query_capability(
                        &name,
                        Some((vector(point)?, radius as f32)),
                        usize::try_from(limit).unwrap_or(0),
                    )?
                    .into_iter()
                    .map(Dynamic::from)
                    .collect())
            })
        },
    );
}
