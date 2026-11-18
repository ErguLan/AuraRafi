//! Explicit capsule character operations shared by script adapters.
use glam::Vec3;
use raf_core::scene::{Aabb, CharacterController, ColliderType, RigidBodyType};

use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};

impl NodeHandle {
    /// Configure an opt-in capsule in world meters, including the rounded ends.
    pub fn configure_character(
        &self,
        ctx: &mut ScriptContext<'_>,
        radius: f32,
        height: f32,
        max_slope_degrees: f32,
        max_step_height: f32,
        jump_height: f32,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let controller = CharacterController {
            radius,
            height,
            max_slope_degrees,
            max_step_height,
            jump_height,
            ..CharacterController::default()
        };
        controller
            .validate()
            .map_err(|message| ScriptError::InvalidArgument(message.into()))?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?;
        node.character_controller = Some(controller);
        node.rigid_body.enabled = true;
        node.rigid_body.body_type = RigidBodyType::Dynamic;
        node.rigid_body.use_gravity = true;
        node.rigid_body.is_character = true;
        node.rigid_body.is_trigger = false;
        node.collider.collider_type = ColliderType::Aabb;
        node.collider.aabb = Aabb {
            min: Vec3::new(-radius, -height * 0.5, -radius),
            max: Vec3::new(radius, height * 0.5, radius),
        };
        node.collider.offset = Vec3::ZERO;
        Ok(())
    }

    /// Set desired world velocity. The runtime projects it onto the gravity plane.
    pub fn character_set_velocity(
        &self,
        ctx: &mut ScriptContext<'_>,
        velocity: Vec3,
    ) -> ScriptResult<()> {
        if !velocity.is_finite() {
            return Err(ScriptError::InvalidArgument(
                "Character velocity must be finite".into(),
            ));
        }
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?;
        let controller = node.character_controller.as_mut().ok_or_else(|| {
            ScriptError::InvalidArgument("Character controller is not configured".into())
        })?;
        controller.state.desired_velocity = velocity;
        Ok(())
    }

    /// Request one jump on the next physics tick, provided support exists.
    pub fn character_jump(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?;
        let controller = node.character_controller.as_mut().ok_or_else(|| {
            ScriptError::InvalidArgument("Character controller is not configured".into())
        })?;
        controller.state.jump_requested = true;
        Ok(())
    }

    pub fn character_grounded(&self, ctx: &ScriptContext<'_>) -> ScriptResult<bool> {
        let node = ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?;
        Ok(node
            .character_controller
            .as_ref()
            .ok_or_else(|| {
                ScriptError::InvalidArgument("Character controller is not configured".into())
            })?
            .state
            .grounded)
    }
}
