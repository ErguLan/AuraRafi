//! Opaque handle to a scene entity.
//!
//! Scripts hold `NodeHandle` values, never raw `&mut SceneNode`. The handle
//! is a cheap, `Copy` ID. Every method takes a `&mut ScriptContext` which
//! validates the ID before touching the scene graph.
//!
//! This is the Roblox `Part` equivalent: `script.Parent.Part1` becomes
//! `get_node("Part1")`.

use crate::errors::ScriptError;
use crate::host_api::ScriptContext;
use crate::value::ScriptValue;
use crate::ScriptResult;
use glam::Vec3;
use raf_core::scene::graph::{NodeColor, SceneNodeId};
use raf_core::scene::VariableValue;
use uuid::Uuid;

/// Version of the Host API. Bump on breaking changes.
pub const HOST_API_VERSION: u32 = 1;

/// An opaque reference to a scene entity, safe to store in scripts.
///
/// Internally this is just a `SceneNodeId` packed into a `u64`. If the
/// entity is destroyed, the next Host API call returns `InvalidHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle {
    id: u64,
    instance_id: u64,
    identity: Option<Uuid>,
}

impl NodeHandle {
    /// Create a handle from a scene node id.
    pub fn from_scene_id(id: SceneNodeId) -> Self {
        Self {
            id: id.0 as u64,
            instance_id: 0,
            identity: None,
        }
    }

    /// Create a handle from a raw u64 (used by Rhai/WASM interop).
    pub fn from_raw(raw: u64) -> Self {
        Self {
            id: raw,
            instance_id: 0,
            identity: None,
        }
    }

    /// Missing references never alias index zero or any active entity.
    pub fn invalid() -> Self {
        Self::from_raw(u64::MAX)
    }

    pub(crate) fn scoped(id: SceneNodeId, ctx: &ScriptContext<'_>) -> Self {
        Self {
            id: id.0 as u64,
            instance_id: ctx.instance_id,
            identity: ctx.scene.get(id).map(|node| node.uuid),
        }
    }

    pub(crate) fn resolve(&self, ctx: &ScriptContext<'_>) -> ScriptResult<SceneNodeId> {
        let id = self.to_scene_id();
        if self.id == u64::MAX
            || self.instance_id != ctx.instance_id
            || !ctx.scene.is_valid_node(id)
            || self
                .identity
                .is_some_and(|identity| ctx.scene.get(id).map(|n| n.uuid) != Some(identity))
        {
            return Err(ScriptError::InvalidHandle(self.id));
        }
        Ok(id)
    }

    /// Convert back to a SceneNodeId for internal use.
    pub fn to_scene_id(&self) -> SceneNodeId {
        SceneNodeId(self.id as usize)
    }

    /// Raw id for serialization across the WASM boundary.
    pub fn raw(&self) -> u64 {
        self.id
    }

    /// Check if the entity still exists in the scene.
    pub fn is_valid(&self, ctx: &ScriptContext<'_>) -> bool {
        self.resolve(ctx).is_ok()
    }

    /// Set world-space position in meters.
    pub fn set_position(
        &self,
        ctx: &mut ScriptContext<'_>,
        x: f32,
        y: f32,
        z: f32,
    ) -> ScriptResult<()> {
        ensure_finite([x, y, z])?;
        let id = self.resolve(ctx)?;
        let parent = ctx.scene.get(id).and_then(|node| node.parent);
        let position = if let Some(parent) = parent {
            let matrix = ctx.scene.world_matrix(parent);
            if matrix.determinant().abs() < 1e-8 {
                return Err(ScriptError::InvalidArgument(
                    "parent transform is singular".into(),
                ));
            }
            matrix.inverse().transform_point3(Vec3::new(x, y, z))
        } else {
            Vec3::new(x, y, z)
        };
        ensure_finite(position.to_array())?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.position = position;
        Ok(())
    }

    /// Set euler rotation in radians.
    pub fn set_rotation(
        &self,
        ctx: &mut ScriptContext<'_>,
        x: f32,
        y: f32,
        z: f32,
    ) -> ScriptResult<()> {
        ensure_finite([x, y, z])?;
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        let rotation = Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees());
        ensure_finite(rotation.to_array())?;
        node.rotation = rotation;
        Ok(())
    }

    /// Set scale (multiplier, 1.0 = original).
    pub fn set_scale(
        &self,
        ctx: &mut ScriptContext<'_>,
        x: f32,
        y: f32,
        z: f32,
    ) -> ScriptResult<()> {
        ensure_finite([x, y, z])?;
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.scale = Vec3::new(x, y, z);
        Ok(())
    }

    /// Get world-space position in meters.
    pub fn get_position(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let id = self.resolve(ctx)?;
        let position = ctx
            .scene
            .world_matrix(id)
            .transform_point3(Vec3::ZERO)
            .to_array();
        ensure_finite(position)?;
        Ok(position)
    }

    /// Get euler rotation in radians.
    pub fn get_rotation(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let node = ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        Ok([
            node.rotation.x.to_radians(),
            node.rotation.y.to_radians(),
            node.rotation.z.to_radians(),
        ])
    }

    /// Get scale.
    pub fn get_scale(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let node = ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        Ok(node.scale.to_array())
    }

    /// Move by a delta in meters.
    pub fn move_by(
        &self,
        ctx: &mut ScriptContext<'_>,
        dx: f32,
        dy: f32,
        dz: f32,
    ) -> ScriptResult<()> {
        ensure_finite([dx, dy, dz])?;
        let [x, y, z] = self.get_position(ctx)?;
        self.set_position(ctx, x + dx, y + dy, z + dz)
    }

    /// Rotate by a delta in radians.
    pub fn rotate_by(
        &self,
        ctx: &mut ScriptContext<'_>,
        dx: f32,
        dy: f32,
        dz: f32,
    ) -> ScriptResult<()> {
        ensure_finite([dx, dy, dz])?;
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        let rotation = node.rotation + Vec3::new(dx.to_degrees(), dy.to_degrees(), dz.to_degrees());
        ensure_finite(rotation.to_array())?;
        node.rotation = rotation;
        Ok(())
    }

    /// Set RGBA color (0-255 per channel).
    pub fn set_color(
        &self,
        ctx: &mut ScriptContext<'_>,
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.color = NodeColor::rgba(r, g, b, a);
        Ok(())
    }

    /// Set visibility.
    pub fn set_visible(&self, ctx: &mut ScriptContext<'_>, visible: bool) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.visible = visible;
        Ok(())
    }

    /// Set the entity name.
    pub fn set_name(&self, ctx: &mut ScriptContext<'_>, name: &str) -> ScriptResult<()> {
        if name.is_empty() || name.len() > 256 {
            return Err(ScriptError::InvalidArgument(
                "name must contain 1..256 bytes".into(),
            ));
        }
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.name = name.to_string();
        Ok(())
    }

    /// Get a custom property by key.
    pub fn get_property(&self, ctx: &ScriptContext<'_>, key: &str) -> ScriptResult<ScriptValue> {
        let node = ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        match key {
            "name" => Ok(ScriptValue::String(node.name.clone())),
            "visible" => Ok(ScriptValue::Bool(node.visible)),
            "position" => Ok(ScriptValue::Vec3(self.get_position(ctx)?)),
            "rotation" => Ok(ScriptValue::Vec3(self.get_rotation(ctx)?)),
            "scale" => Ok(ScriptValue::Vec3(node.scale.to_array())),
            "color" => {
                let c = node.color;
                Ok(ScriptValue::Color([c.r, c.g, c.b, c.a]))
            }
            _ => Ok(match node.get_variable(key) {
                Some(VariableValue::Bool(value)) => ScriptValue::Bool(*value),
                Some(VariableValue::Number(value)) => ScriptValue::Float(*value),
                Some(VariableValue::Text(value)) => ScriptValue::String(value.clone()),
                None => ScriptValue::None,
            }),
        }
    }

    /// Custom parameters use the persisted scene variable contract.
    pub fn set_property(
        &self,
        ctx: &mut ScriptContext<'_>,
        key: &str,
        value: ScriptValue,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        if key.is_empty()
            || key.len() > 128
            || matches!(
                key,
                "name" | "visible" | "position" | "rotation" | "scale" | "color"
            )
        {
            return Err(ScriptError::InvalidArgument("invalid variable name".into()));
        }
        let value = match value {
            ScriptValue::Bool(value) => VariableValue::Bool(value),
            ScriptValue::Int(value) => VariableValue::Number(value as f32),
            ScriptValue::Float(value) if value.is_finite() => VariableValue::Number(value),
            ScriptValue::String(value) if value.len() <= 4096 => VariableValue::Text(value),
            _ => {
                return Err(ScriptError::InvalidArgument(
                    "variable requires Bool, finite Number or bounded Text".into(),
                ))
            }
        };
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.id))?;
        if node.get_variable(key).is_none() && node.variables.len() >= 128 {
            return Err(ScriptError::InvalidArgument(
                "entity variable limit exceeded".into(),
            ));
        }
        node.set_variable(key, value);
        Ok(())
    }
}

fn ensure_finite(values: [f32; 3]) -> ScriptResult<()> {
    if values.iter().all(|value| value.is_finite()) {
        Ok(())
    } else {
        Err(ScriptError::InvalidArgument(
            "transform values must be finite".into(),
        ))
    }
}
