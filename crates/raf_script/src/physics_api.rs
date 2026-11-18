//! Language-neutral physics controls. All vectors are world-space SI values.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};
use glam::Vec3;
use raf_core::scene::{Aabb, ColliderType, RigidBodyType};
use uuid::Uuid;

fn invalid(message: &str) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}

impl ScriptContext<'_> {
    pub fn set_world_gravity(&mut self, gravity: Vec3) -> ScriptResult<()> {
        if !self.view.advanced_physics {
            return Err(invalid("World gravity requires advanced physics"));
        }
        if !gravity.is_finite() || gravity.abs().max_element() > 1000.0 {
            return Err(invalid(
                "World gravity must be finite and within the supported range",
            ));
        }
        self.view.world_gravity = Some(gravity);
        Ok(())
    }
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
    pub fn apply_force(&self, ctx: &mut ScriptContext<'_>, force: Vec3) -> ScriptResult<()> {
        let body = self.dynamic_body(ctx)?;
        accumulate(&mut body.force, force)
    }

    pub fn apply_torque(&self, ctx: &mut ScriptContext<'_>, torque: Vec3) -> ScriptResult<()> {
        let body = self.dynamic_body(ctx)?;
        accumulate(&mut body.torque, torque)
    }

    pub fn apply_impulse_at(
        &self,
        ctx: &mut ScriptContext<'_>,
        impulse: Vec3,
        point: Vec3,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get(id)
            .ok_or_else(|| invalid("Body was deleted"))?;
        let center = ctx
            .scene
            .world_matrix(id)
            .transform_point3(node.collider.aabb.center() + node.collider.offset);
        let body = self.dynamic_body(ctx)?;
        let linear = body.impulse + impulse;
        let angular = body.angular_impulse + (point - center).cross(impulse);
        if !linear.is_finite() || !angular.is_finite() {
            return Err(invalid("Impulse at point must be finite"));
        }
        body.impulse = linear;
        body.angular_impulse = angular;
        Ok(())
    }

    fn dynamic_body<'a>(
        &self,
        ctx: &'a mut ScriptContext<'_>,
    ) -> ScriptResult<&'a mut raf_core::scene::RigidBody> {
        if !ctx.view.advanced_physics {
            return Err(invalid("This operation requires advanced physics"));
        }
        let id = self.resolve(ctx)?;
        let body = &mut ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| invalid("Body was deleted"))?
            .rigid_body;
        if !body.enabled || body.body_type != RigidBodyType::Dynamic || body.is_character {
            return Err(invalid("An enabled dynamic rigid body is required"));
        }
        Ok(body)
    }
    /// Marks the body as a character for lightweight simulation.
    pub fn set_character_body(
        &self,
        ctx: &mut ScriptContext<'_>,
        enabled: bool,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        ctx.scene
            .get_mut(id)
            .ok_or(ScriptError::InvalidHandle(self.raw()))?
            .rigid_body
            .is_character = enabled;
        Ok(())
    }
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
        if kind == RigidBodyType::Static && body.body_type != RigidBodyType::Static {
            body.velocity = Vec3::ZERO;
            body.angular_velocity = Vec3::ZERO;
            body.force = Vec3::ZERO;
            body.torque = Vec3::ZERO;
            body.impulse = Vec3::ZERO;
            body.angular_impulse = Vec3::ZERO;
        }
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

fn accumulate(total: &mut Vec3, value: Vec3) -> ScriptResult<()> {
    let next = *total + value;
    if !next.is_finite() {
        return Err(invalid("Physics accumulator must be finite"));
    }
    *total = next;
    Ok(())
}
