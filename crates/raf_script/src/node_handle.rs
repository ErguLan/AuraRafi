//! Opaque handle to a scene entity.
//!
//! Scripts hold `NodeHandle` values, never raw `&mut SceneNode`. The handle
//! is a cheap, `Copy` ID. Every method takes a `&mut ScriptContext` which
//! validates the ID before touching the scene graph.
//!
//! Attachment ownership and hierarchy parenting are distinct Host API concepts.

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
/// Internal references validate the runtime instance, slot and entity UUID.
/// WASM guests use a separate bounded token table, never these raw indices.
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

    /// Legacy engine-only constructor. Never use to decode a WASM guest token.
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

    /// Internal slot identifier, NOT a safe serialized WASM guest reference.
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

    fn particle_component_mut<'a>(
        &self,
        ctx: &'a mut ScriptContext<'_>,
    ) -> ScriptResult<&'a mut raf_core::scene::ParticleSystemComponent> {
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or_else(|| ScriptError::InvalidHandle(self.id))?
            .particle_system
            .as_mut()
            .ok_or_else(|| ScriptError::InvalidArgument("entity has no particle system".into()))
    }

    pub fn particle_play(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        self.particle_component_mut(ctx)?.playback = raf_core::scene::ParticlePlayback::Playing;
        Ok(())
    }

    pub fn particle_pause(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        self.particle_component_mut(ctx)?.playback = raf_core::scene::ParticlePlayback::Paused;
        Ok(())
    }

    pub fn particle_stop(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        self.particle_component_mut(ctx)?.playback = raf_core::scene::ParticlePlayback::Stopped;
        Ok(())
    }

    pub fn particle_freeze(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        self.particle_component_mut(ctx)?.playback = raf_core::scene::ParticlePlayback::Frozen;
        Ok(())
    }

    /// Requests extra one-shot particles on the next rendered frame.
    pub fn particle_emit(&self, ctx: &mut ScriptContext<'_>, count: u32) -> ScriptResult<()> {
        if count > 100_000 {
            return Err(ScriptError::InvalidArgument(
                "emit count is capped at 100000".into(),
            ));
        }
        self.particle_component_mut(ctx)?.request_emit(count);
        Ok(())
    }

    pub fn particle_set_rate(&self, ctx: &mut ScriptContext<'_>, scale: f32) -> ScriptResult<()> {
        if !scale.is_finite() {
            return Err(ScriptError::InvalidArgument(
                "particle rate scale must be finite".into(),
            ));
        }
        self.particle_component_mut(ctx)?.rate_scale = scale.clamp(0.0, 8.0);
        Ok(())
    }

    pub fn particle_set_time_scale(
        &self,
        ctx: &mut ScriptContext<'_>,
        scale: f32,
    ) -> ScriptResult<()> {
        if !scale.is_finite() {
            return Err(ScriptError::InvalidArgument(
                "particle time scale must be finite".into(),
            ));
        }
        self.particle_component_mut(ctx)?.time_scale = scale.clamp(0.0, 8.0);
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
            "casts_shadows" => Ok(ScriptValue::Bool(node.casts_shadows)),
            "receives_shadows" => Ok(ScriptValue::Bool(node.receives_shadows)),
            "position" => Ok(ScriptValue::Vec3(self.get_position(ctx)?)),
            "rotation" => Ok(ScriptValue::Vec3(self.get_rotation(ctx)?)),
            "scale" => Ok(ScriptValue::Vec3(node.scale.to_array())),
            "color" => {
                let c = node.color;
                Ok(ScriptValue::Color([c.r, c.g, c.b, c.a]))
            }
            "audio.enabled" => Ok(ScriptValue::Bool(node.audio_source.enabled)),
            "audio.clip" => Ok(ScriptValue::String(node.audio_source.clip.clone())),
            "audio.volume" => Ok(ScriptValue::Float(node.audio_source.volume)),
            "audio.looping" => Ok(ScriptValue::Bool(node.audio_source.looping)),
            "audio.autoplay" => Ok(ScriptValue::Bool(node.audio_source.autoplay)),
            "audio.bus" => Ok(ScriptValue::String(
                node.audio_source.bus.slug().to_string(),
            )),
            "audio.spatial" => Ok(ScriptValue::Bool(node.audio_source.spatial)),
            "audio.min_distance" => Ok(ScriptValue::Float(node.audio_source.min_distance)),
            "audio.max_distance" => Ok(ScriptValue::Float(node.audio_source.max_distance)),
            "particles.playing" | "particles.paused" | "particles.stopped" | "particles.frozen" => {
                Ok(match node.particle_system.as_ref().map(|p| p.playback) {
                    Some(playback) => ScriptValue::Bool(match key {
                        "particles.playing" => {
                            playback == raf_core::scene::ParticlePlayback::Playing
                        }
                        "particles.paused" => playback == raf_core::scene::ParticlePlayback::Paused,
                        "particles.stopped" => {
                            playback == raf_core::scene::ParticlePlayback::Stopped
                        }
                        _ => playback == raf_core::scene::ParticlePlayback::Frozen,
                    }),
                    None => ScriptValue::None,
                })
            }
            "particles.time_scale" => Ok(node
                .particle_system
                .as_ref()
                .map(|p| ScriptValue::Float(p.time_scale))
                .unwrap_or(ScriptValue::None)),
            "particles.rate_scale" => Ok(node
                .particle_system
                .as_ref()
                .map(|p| ScriptValue::Float(p.rate_scale))
                .unwrap_or(ScriptValue::None)),
            "particles.asset" => Ok(node
                .particle_system
                .as_ref()
                .map(|p| ScriptValue::String(p.asset.clone()))
                .unwrap_or(ScriptValue::None)),
            _ => Ok(match node.get_variable(key) {
                Some(VariableValue::Bool(value)) => ScriptValue::Bool(*value),
                Some(VariableValue::Number(value)) => ScriptValue::Float(*value),
                Some(VariableValue::Text(value)) => ScriptValue::String(value.clone()),
                // Serialized UUIDs are data, never fabricated runtime handles.
                Some(VariableValue::Entity(value)) => value
                    .map(|id| ScriptValue::String(id.to_string()))
                    .unwrap_or(ScriptValue::None),
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
        if matches!(key, "casts_shadows" | "receives_shadows") {
            let ScriptValue::Bool(value) = value else {
                return Err(ScriptError::InvalidArgument(
                    "Shadow participation requires Bool".into(),
                ));
            };
            let node = ctx
                .scene
                .get_mut(id)
                .ok_or(ScriptError::InvalidHandle(self.id))?;
            if key == "casts_shadows" {
                node.casts_shadows = value;
            } else {
                node.receives_shadows = value;
            }
            return Ok(());
        }
        if let Some(audio_key) = key.strip_prefix("audio.") {
            let node = ctx
                .scene
                .get_mut(id)
                .ok_or(ScriptError::InvalidHandle(self.id))?;
            match (audio_key, value) {
                ("enabled", ScriptValue::Bool(value)) => node.audio_source.enabled = value,
                ("looping", ScriptValue::Bool(value)) => node.audio_source.looping = value,
                ("autoplay", ScriptValue::Bool(value)) => node.audio_source.autoplay = value,
                ("spatial", ScriptValue::Bool(value)) => node.audio_source.spatial = value,
                ("clip", ScriptValue::String(value)) => node.audio_source.clip = value,
                ("bus", ScriptValue::String(value)) => {
                    node.audio_source.bus = raf_core::scene::AudioBus::from_slug(&value)
                        .ok_or_else(|| ScriptError::InvalidArgument("unknown audio bus".into()))?;
                }
                ("volume", ScriptValue::Float(value)) => {
                    node.audio_source.volume = value.clamp(0.0, 1.0)
                }
                ("volume", ScriptValue::Int(value)) => {
                    node.audio_source.volume = (value as f32).clamp(0.0, 1.0)
                }
                ("min_distance", ScriptValue::Float(value)) => {
                    node.audio_source.min_distance = value.max(0.001)
                }
                ("max_distance", ScriptValue::Float(value)) => {
                    node.audio_source.max_distance = value.max(0.002)
                }
                _ => {
                    return Err(ScriptError::InvalidArgument(
                        "audio property requires its matching value type".into(),
                    ))
                }
            }
            node.audio_source
                .validate()
                .map_err(|error| ScriptError::InvalidArgument(error.into()))?;
            return Ok(());
        }
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
