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

/// Version of the Host API. Bump on breaking changes.
pub const HOST_API_VERSION: u32 = 1;

/// An opaque reference to a scene entity, safe to store in scripts.
///
/// Internally this is just a `SceneNodeId` packed into a `u64`. If the
/// entity is destroyed, the next Host API call returns `InvalidHandle`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeHandle {
    id: u64,
}

impl NodeHandle {
    /// Create a handle from a scene node id.
    pub fn from_scene_id(id: SceneNodeId) -> Self {
        Self { id: id.0 as u64 }
    }

    /// Create a handle from a raw u64 (used by Rhai/WASM interop).
    pub fn from_raw(raw: u64) -> Self {
        Self { id: raw }
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
        ctx.scene.get(self.to_scene_id()).is_some()
    }

    /// Set world-space position in meters.
    pub fn set_position(&self, ctx: &mut ScriptContext<'_>, x: f32, y: f32, z: f32) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.position = Vec3::new(x, y, z);
        Ok(())
    }

    /// Set euler rotation in radians.
    pub fn set_rotation(&self, ctx: &mut ScriptContext<'_>, x: f32, y: f32, z: f32) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.rotation = Vec3::new(x, y, z);
        Ok(())
    }

    /// Set scale (multiplier, 1.0 = original).
    pub fn set_scale(&self, ctx: &mut ScriptContext<'_>, x: f32, y: f32, z: f32) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.scale = Vec3::new(x, y, z);
        Ok(())
    }

    /// Get world-space position in meters.
    pub fn get_position(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let node = ctx
            .scene
            .get(self.to_scene_id())
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        Ok(node.position.to_array())
    }

    /// Get euler rotation in radians.
    pub fn get_rotation(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let node = ctx
            .scene
            .get(self.to_scene_id())
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        Ok(node.rotation.to_array())
    }

    /// Get scale.
    pub fn get_scale(&self, ctx: &ScriptContext<'_>) -> ScriptResult<[f32; 3]> {
        let node = ctx
            .scene
            .get(self.to_scene_id())
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        Ok(node.scale.to_array())
    }

    /// Move by a delta in meters.
    pub fn move_by(&self, ctx: &mut ScriptContext<'_>, dx: f32, dy: f32, dz: f32) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.position += Vec3::new(dx, dy, dz);
        Ok(())
    }

    /// Rotate by a delta in radians.
    pub fn rotate_by(&self, ctx: &mut ScriptContext<'_>, dx: f32, dy: f32, dz: f32) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.rotation += Vec3::new(dx, dy, dz);
        Ok(())
    }

    /// Set RGBA color (0-255 per channel).
    pub fn set_color(&self, ctx: &mut ScriptContext<'_>, r: u8, g: u8, b: u8, a: u8) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.color = NodeColor::rgba(r, g, b, a);
        Ok(())
    }

    /// Set visibility.
    pub fn set_visible(&self, ctx: &mut ScriptContext<'_>, visible: bool) -> ScriptResult<()> {
        let id = self.to_scene_id();
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        node.visible = visible;
        Ok(())
    }

    /// Set the entity name.
    pub fn set_name(&self, ctx: &mut ScriptContext<'_>, name: &str) -> ScriptResult<()> {
        let id = self.to_scene_id();
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
            .get(self.to_scene_id())
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?;
        match key {
            "name" => Ok(ScriptValue::String(node.name.clone())),
            "visible" => Ok(ScriptValue::Bool(node.visible)),
            "position" => Ok(ScriptValue::Vec3(node.position.to_array())),
            "rotation" => Ok(ScriptValue::Vec3(node.rotation.to_array())),
            "scale" => Ok(ScriptValue::Vec3(node.scale.to_array())),
            "color" => {
                let c = node.color;
                Ok(ScriptValue::Color([c.r, c.g, c.b, c.a]))
            }
            _ => Ok(ScriptValue::None),
        }
    }
}
