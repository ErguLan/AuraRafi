//! Language-neutral physics controls. All vectors are world-space SI values.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, RigidBodyType};
use uuid::Uuid;

fn invalid(message: &str) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}

/// Typed engine events cannot be forged through the authored event queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PhysicsEventKind {
    CollisionEnter,
    CollisionStay,
    CollisionExit,
    TriggerEnter,
    TriggerStay,
    TriggerExit,
}
impl PhysicsEventKind {
    pub const ALL: [Self; 6] = [
        Self::CollisionEnter,
        Self::CollisionStay,
        Self::CollisionExit,
        Self::TriggerEnter,
        Self::TriggerStay,
        Self::TriggerExit,
    ];
    pub fn hook(self) -> &'static str {
        match self {
            Self::CollisionEnter => "on_collision_enter",
            Self::CollisionStay => "on_collision_stay",
            Self::CollisionExit => "on_collision_exit",
            Self::TriggerEnter => "on_trigger_enter",
            Self::TriggerStay => "on_trigger_stay",
            Self::TriggerExit => "on_trigger_exit",
        }
    }
    pub fn name(self) -> &'static str {
        self.hook().trim_start_matches("on_")
    }
}
#[derive(Debug, Clone, Copy)]
pub struct PhysicsEvent {
    pub kind: PhysicsEventKind,
    pub receiver: Uuid,
    pub other: Uuid,
}

impl NodeHandle {
    pub fn velocity(&self, ctx: &ScriptContext<'_>) -> ScriptResult<Vec3> {
        Ok(ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body
            .velocity)
    }
    pub fn set_velocity(&self, ctx: &mut ScriptContext<'_>, velocity: Vec3) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        if !velocity.is_finite() {
            return Err(invalid("Velocity must be finite (m/s)"));
        }
        ctx.scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body
            .velocity = velocity;
        Ok(())
    }
    pub fn add_velocity(&self, ctx: &mut ScriptContext<'_>, delta: Vec3) -> ScriptResult<()> {
        let velocity = self.velocity(ctx)? + delta;
        self.set_velocity(ctx, velocity)
    }
    pub fn apply_impulse(&self, ctx: &mut ScriptContext<'_>, impulse: Vec3) -> ScriptResult<()> {
        let body = &ctx
            .scene
            .get(self.resolve(ctx)?)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body;
        if !body.enabled || body.body_type != RigidBodyType::Dynamic {
            return Err(invalid("Impulse requires an enabled dynamic body"));
        }
        if !body.mass.is_finite() || body.mass <= 0.0 {
            return Err(invalid("Mass must be positive (kg)"));
        }
        let delta = impulse / body.mass;
        self.add_velocity(ctx, delta)
    }
    /// Explicit configuration; never adds a collider or selects a game role.
    pub fn configure_body(
        &self,
        ctx: &mut ScriptContext<'_>,
        kind: RigidBodyType,
        gravity: bool,
        trigger: bool,
        mass: f32,
        damping: f32,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        if !mass.is_finite() || mass <= 0.0 || !damping.is_finite() || damping < 0.0 {
            return Err(invalid(
                "Body requires positive finite mass and nonnegative finite damping",
            ));
        }
        let body = &mut ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body;
        body.body_type = kind;
        body.use_gravity = gravity;
        body.is_trigger = trigger;
        body.mass = mass;
        body.damping = damping;
        body.enabled = true;
        Ok(())
    }
    pub fn set_physics_enabled(
        &self,
        ctx: &mut ScriptContext<'_>,
        enabled: bool,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body
            .enabled = enabled;
        Ok(())
    }
    /// Bounds are local-space; transforms and parent transforms are applied by physics.
    pub fn set_aabb_collider(
        &self,
        ctx: &mut ScriptContext<'_>,
        min: Vec3,
        max: Vec3,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        if !min.is_finite() || !max.is_finite() || min.cmpge(max).any() {
            return Err(invalid("Collider requires finite min < max on every axis"));
        }
        let collider = &mut ctx
            .scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .collider;
        collider.collider_type = ColliderType::Aabb;
        collider.aabb = Aabb { min, max };
        collider.offset = Vec3::ZERO;
        Ok(())
    }
    pub fn remove_collider(&self, ctx: &mut ScriptContext<'_>) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .collider
            .collider_type = ColliderType::None;
        Ok(())
    }
}
