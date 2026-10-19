//! Shared environment Host API. Local overrides never mutate the authored world.
use crate::{view::ScriptRole, ScriptContext, ScriptError, ScriptResult, ScriptValue};
use serde_json::{json, Value};
fn invalid(message: impl Into<String>) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}
fn json_value(value: ScriptValue) -> ScriptResult<Value> {
    Ok(match value {
        ScriptValue::Bool(v) => json!(v),
        ScriptValue::Int(v) => json!(v),
        ScriptValue::Float(v) if v.is_finite() && v >= 0.0 && v <= 255.0 && v.fract() == 0.0 => {
            json!(v as u8)
        }
        ScriptValue::Float(v) if v.is_finite() => json!(v),
        ScriptValue::Vec3(v) => Value::Array(
            v.into_iter()
                .map(|v| json_value(ScriptValue::Float(v)))
                .collect::<ScriptResult<_>>()?,
        ),
        ScriptValue::String(v) if v.len() <= 32 => json!(v),
        ScriptValue::Color(v) => json!([v[0], v[1], v[2]]),
        ScriptValue::List(v) if v.len() == 3 => {
            Value::Array(v.into_iter().map(json_value).collect::<ScriptResult<_>>()?)
        }
        _ => {
            return Err(invalid(
                "Lighting expects boolean, finite number, choice, or [r,g,b] bytes",
            ))
        }
    })
}
fn script_value(value: Value) -> ScriptResult<ScriptValue> {
    Ok(match value {
        Value::Bool(v) => ScriptValue::Bool(v),
        Value::String(v) => ScriptValue::String(v),
        Value::Number(v) => {
            ScriptValue::Float(v.as_f64().ok_or_else(|| invalid("Invalid number"))? as f32)
        }
        Value::Array(values) => ScriptValue::List(
            values
                .into_iter()
                .map(|v| {
                    Ok(ScriptValue::Int(
                        v.as_u64().ok_or_else(|| invalid("Invalid color"))? as i64,
                    ))
                })
                .collect::<ScriptResult<_>>()?,
        ),
        _ => return Err(invalid("Unknown environment value")),
    })
}
impl ScriptContext<'_> {
    pub fn get_environment(&self, path: &str) -> ScriptResult<ScriptValue> {
        script_value(
            self.scene
                .environment()
                .get_property(path)
                .map_err(invalid)?,
        )
    }
    pub fn set_environment(&mut self, path: &str, value: ScriptValue) -> ScriptResult<()> {
        self.check_budget()?;
        if self.view.role == ScriptRole::Client {
            return Err(invalid(
                "Client scripts cannot write shared world lighting; use set_view_environment",
            ));
        }
        let patch = serde_json::Map::from_iter([(path.into(), json_value(value)?)]);
        let next = self.scene.environment().patched(&patch).map_err(invalid)?;
        self.scene.set_environment(next);
        Ok(())
    }
    pub fn get_view_environment(&self, path: &str) -> ScriptResult<ScriptValue> {
        if self.view.role == ScriptRole::Server {
            return Err(invalid("Server scripts have no local presentation"));
        }
        script_value(
            self.view
                .environment(*self.scene.environment())
                .get_property(path)
                .map_err(invalid)?,
        )
    }
    pub fn set_view_environment(&mut self, path: &str, value: ScriptValue) -> ScriptResult<()> {
        self.check_budget()?;
        if self.view.role == ScriptRole::Server {
            return Err(invalid("Server scripts cannot write local presentation"));
        }
        // The view is a presentation override, not another clock or source of shared time.
        if path.starts_with("clock.") {
            return Err(invalid("View overrides cannot own the simulation clock"));
        }
        let mut patch = self.view.environment_overrides.clone();
        patch.insert(path.into(), json_value(value)?);
        self.scene.environment().patched(&patch).map_err(invalid)?;
        if patch != self.view.environment_overrides {
            self.view.environment_overrides = patch;
            self.view.environment_revision = self.view.environment_revision.wrapping_add(1);
        }
        Ok(())
    }
    pub fn clear_view_environment(&mut self, path: Option<&str>) -> ScriptResult<()> {
        self.check_budget()?;
        if self.view.role == ScriptRole::Server {
            return Err(invalid("Server scripts have no local presentation"));
        }
        let changed = if let Some(path) = path {
            raf_core::environment::environment_property(path).map_err(invalid)?;
            self.view.environment_overrides.remove(path).is_some()
        } else {
            let changed = !self.view.environment_overrides.is_empty();
            self.view.environment_overrides.clear();
            changed
        };
        if changed {
            self.view.environment_revision = self.view.environment_revision.wrapping_add(1);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backends::rhai_backend as rhai;
    use crate::{
        host_api::ScriptEventQueue, view::RuntimeViewState, AudioCommandQueue, InputSnapshot,
        TimeInfo,
    };
    #[test]
    fn lighting_rhai_and_nodes_use_the_same_host_contract_and_view_is_isolated() {
        let engine = rhai::create_engine(100_000);
        let mut scene = raf_core::SceneGraph::new();
        let mut view = RuntimeViewState::default();
        let input = InputSnapshot::default();
        let mut audio = AudioCommandQueue::default();
        let mut events = ScriptEventQueue::default();
        let mut ctx = ScriptContext {
            scene: &mut scene,
            view: &mut view,
            input: &input,
            audio: &mut audio,
            events: &mut events,
            time: TimeInfo::default(),
            instance_id: 1,
            owner: None,
            behavior_source: None,
            entity_limit: 1000,
            cancellation: None,
            deadline: None,
        };
        let script = rhai::compile_source(
            &engine,
            "lighting.rhai",
            r#"
            set_environment("mode","scene"); set_environment("sun.intensity",1.5);
            set_environment("shadows.enabled",true); set_environment("shadows.resolution",512);
            set_view_environment("shadows.distance",12.0);
            set_view_environment("sun.color",[255,64,32]);
            if get_environment("sun.intensity") != 1.5 { throw "global mismatch"; }
        "#,
        )
        .unwrap();
        let result = rhai::initialize_scope(&engine, &script, &mut ctx, &mut ::rhai::Scope::new());
        assert!(result.success, "{:?}", result.errors);
        assert_eq!(ctx.scene.environment().sun.intensity, 1.5);
        assert!(ctx.scene.environment().shadows.enabled);
        assert_eq!(
            ctx.view
                .environment(*ctx.scene.environment())
                .shadows
                .distance,
            12.0
        );
        let id = ctx
            .scene
            .add_root_with_primitive("Caster", raf_core::scene::Primitive::Cube);
        let handle = crate::node_handle::NodeHandle::scoped(id, &ctx);
        handle
            .set_property(&mut ctx, "casts_shadows", ScriptValue::Bool(false))
            .unwrap();
        assert!(matches!(
            handle.get_property(&ctx, "casts_shadows").unwrap(),
            ScriptValue::Bool(false)
        ));
        let flags = rhai::compile_source(
            &engine,
            "shadows.rhai",
            r#"
            let caster = entity("Caster");
            set_property(caster,"casts_shadows",true);
            set_property(caster,"receives_shadows",false);
            if get_property(caster,"receives_shadows") { throw "receiver flag mismatch"; }
        "#,
        )
        .unwrap();
        let result = rhai::initialize_scope(&engine, &flags, &mut ctx, &mut ::rhai::Scope::new());
        assert!(result.success, "{:?}", result.errors);
        assert!(ctx.scene.get(id).unwrap().casts_shadows);
        assert!(!ctx.scene.get(id).unwrap().receives_shadows);
        assert_eq!(
            ctx.view.environment(*ctx.scene.environment()).sun.color,
            [255, 64, 32]
        );
        assert_ne!(ctx.scene.environment().sun.color, [255, 64, 32]);
        ctx.view.role = ScriptRole::Client;
        assert!(ctx
            .set_environment("sun.intensity", ScriptValue::Float(2.0))
            .is_err());
        assert!(ctx
            .set_view_environment("sun.intensity", ScriptValue::Float(2.0))
            .is_ok());
        ctx.view.role = ScriptRole::Server;
        assert!(ctx
            .set_view_environment("sun.enabled", ScriptValue::Bool(false))
            .is_err());
        ctx.view.role = ScriptRole::Local;
        ctx.clear_view_environment(None).unwrap();
        let cleared_revision = ctx.view.environment_revision();
        ctx.clear_view_environment(None).unwrap();
        assert_eq!(ctx.view.environment_revision(), cleared_revision);
        assert_eq!(
            ctx.view.environment(*ctx.scene.environment()),
            *ctx.scene.environment()
        );
        for slug in [
            "environment-set-number",
            "environment-set-bool",
            "environment-set-choice",
            "environment-set-color",
        ] {
            let mut graph = raf_nodes::NodeGraph::new("Lighting");
            let start = raf_nodes::catalog::create("on-start").unwrap();
            let node = raf_nodes::catalog::create(slug).unwrap();
            graph.connect(
                start.id,
                start.pins.iter().find(|p| p.name == "Out").unwrap().id,
                node.id,
                node.pins.iter().find(|p| p.name == "In").unwrap().id,
            );
            graph.add_node(start);
            graph.add_node(node);
            let source = raf_nodes::runtime_compiler::to_rhai(&graph).unwrap();
            let script = rhai::compile_source(&engine, "lighting.nodes.ron", &source).unwrap();
            let mut scope = ::rhai::Scope::new();
            assert!(rhai::initialize_scope(&engine, &script, &mut ctx, &mut scope).success);
            let result = rhai::call_hook(&engine, &script, &mut ctx, &mut scope, "on_start", 0.0);
            assert!(result.success, "{slug}: {:?}", result.errors);
        }
        let before = *ctx.scene.environment();
        assert!(ctx
            .set_environment("shadows.resolution", ScriptValue::Float(768.0))
            .is_err());
        assert!(ctx
            .set_environment("sun.intensity", ScriptValue::Float(f32::NAN))
            .is_err());
        assert_eq!(*ctx.scene.environment(), before);
    }
}
