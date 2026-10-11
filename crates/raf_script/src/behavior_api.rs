//! Composable user capabilities and bounded queries; no gameplay rules live here.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult, ScriptValue};
use glam::Vec3;
use raf_core::scene::VariableValue;

fn capability_key(name: &str) -> ScriptResult<String> {
    if name.is_empty()
        || name.len() > 100
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
    {
        return Err(ScriptError::InvalidArgument(
            "capability names require 1..100 ASCII letters, numbers, dots, underscores or hyphens"
                .into(),
        ));
    }
    Ok(format!("capability.{name}"))
}

impl NodeHandle {
    /// Typed shared entity property, usable by global Nodes without an implicit owner.
    pub fn entity_reference(
        &self,
        ctx: &ScriptContext<'_>,
        name: &str,
    ) -> ScriptResult<Option<NodeHandle>> {
        ctx.check_budget()?;
        if name.is_empty() || name.len() > 128 {
            return Err(ScriptError::InvalidArgument(
                "Invalid entity reference name".into(),
            ));
        }
        match ctx
            .scene
            .get(self.resolve(ctx)?)
            .and_then(|node| node.get_variable(name))
        {
            None | Some(VariableValue::Entity(None)) => Ok(None),
            Some(VariableValue::Entity(Some(uuid))) => Ok(ctx.get_node(&uuid.to_string())),
            Some(_) => Err(ScriptError::InvalidArgument(format!(
                "property {name} is not an Entity reference"
            ))),
        }
    }
    /// Capability tags describe user semantics, not security permissions.
    pub fn set_capability(
        &self,
        ctx: &mut ScriptContext<'_>,
        name: &str,
        enabled: bool,
    ) -> ScriptResult<()> {
        self.set_property(ctx, &capability_key(name)?, ScriptValue::Bool(enabled))
    }

    pub fn has_capability(&self, ctx: &ScriptContext<'_>, name: &str) -> ScriptResult<bool> {
        let id = self.resolve(ctx)?;
        let key = capability_key(name)?;
        Ok(matches!(
            ctx.scene.get(id).and_then(|n| n.get_variable(&key)),
            Some(VariableValue::Bool(true))
        ))
    }
}

impl ScriptContext<'_> {
    fn parameter_value(&self, name: &str) -> ScriptResult<Option<&VariableValue>> {
        self.check_budget()?;
        if name.is_empty() || name.len() > 128 {
            return Err(ScriptError::InvalidArgument(
                "parameter name requires 1..128 bytes".into(),
            ));
        }
        let owner = self.owner().ok_or_else(|| {
            ScriptError::InvalidArgument("parameter requires an entity-owned behavior".into())
        })?;
        let node = self
            .scene
            .get(owner.resolve(self)?)
            .expect("resolved owner");
        Ok(self
            .behavior_source
            .and_then(|source| node.behavior_parameters.get(source))
            .and_then(|parameters| parameters.iter().find(|p| p.name == name))
            .map(|p| &p.value)
            .or_else(|| node.get_variable(name)))
    }

    /// Missing, unassigned or deleted references produce no handle, never auto-association.
    pub fn entity_parameter(&self, name: &str) -> ScriptResult<Option<NodeHandle>> {
        match self.parameter_value(name)? {
            None | Some(VariableValue::Entity(None)) => Ok(None),
            Some(VariableValue::Entity(Some(uuid))) => Ok(self.get_node(&uuid.to_string())),
            Some(_) => Err(ScriptError::InvalidArgument(format!(
                "parameter {name} is not an Entity reference"
            ))),
        }
    }

    /// Existing Inspector variables supply parameters; missing values use the caller's default.
    pub fn parameter(&self, name: &str, default: ScriptValue) -> ScriptResult<ScriptValue> {
        if name.is_empty()
            || name.len() > 128
            || !matches!(
                &default,
                ScriptValue::Bool(_) | ScriptValue::Int(_) | ScriptValue::String(_)
            ) && !matches!(&default, ScriptValue::Float(v) if v.is_finite())
        {
            return Err(ScriptError::InvalidArgument(
                "parameter requires a bounded name and Bool, finite Number or Text default".into(),
            ));
        }
        default.validate_event_payload()?;
        let value = match self.parameter_value(name)? {
            Some(VariableValue::Bool(v)) => ScriptValue::Bool(*v),
            Some(VariableValue::Number(v)) => ScriptValue::Float(*v),
            Some(VariableValue::Text(v)) => ScriptValue::String(v.clone()),
            Some(VariableValue::Entity(_)) => {
                return Err(ScriptError::InvalidArgument(format!(
                    "parameter {name} is an Entity reference; use entity_parameter"
                )))
            }
            None => ScriptValue::None,
        };
        value.validate_event_payload()?;
        if value == ScriptValue::None {
            return Ok(default);
        }
        let compatible = matches!(
            (&value, &default),
            (ScriptValue::Bool(_), ScriptValue::Bool(_))
                | (ScriptValue::String(_), ScriptValue::String(_))
                | (
                    ScriptValue::Float(_) | ScriptValue::Int(_),
                    ScriptValue::Float(_) | ScriptValue::Int(_)
                )
        );
        if !compatible {
            return Err(ScriptError::InvalidArgument(format!(
                "parameter {name} has a different type than its default"
            )));
        }
        Ok(value)
    }

    /// Bounded linear scene query. A spatial index can replace this implementation later.
    pub fn query_capability(
        &self,
        name: &str,
        near: Option<(Vec3, f32)>,
        limit: usize,
    ) -> ScriptResult<Vec<NodeHandle>> {
        self.check_budget()?;
        let key = capability_key(name)?;
        if !(1..=256).contains(&limit)
            || near.is_some_and(|(point, radius)| {
                !point.is_finite() || !radius.is_finite() || radius < 0.0 || radius > 1_000_000.0
            })
        {
            return Err(ScriptError::InvalidArgument(
                "query limit requires 1..256; radius requires a finite value in 0..1000000".into(),
            ));
        }
        let mut found = Vec::new();
        for (index, (id, node)) in self.scene.iter().enumerate() {
            if index >= self.entity_limit.min(10_000) {
                break;
            }
            if index % 64 == 0 {
                self.check_budget()?;
            }
            if !self.scene.is_valid_node(id)
                || !matches!(node.get_variable(&key), Some(VariableValue::Bool(true)))
            {
                continue;
            }
            if let Some((point, radius)) = near {
                let world = self.scene.world_matrix(id).transform_point3(Vec3::ZERO);
                if !world.is_finite() || world.distance_squared(point) > radius * radius {
                    continue;
                }
            }
            found.push(NodeHandle::scoped(id, self));
            if found.len() == limit {
                break;
            }
        }
        Ok(found)
    }
}
