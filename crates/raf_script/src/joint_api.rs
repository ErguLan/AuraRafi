//! Host operations for collision filters and authored constraints.
use crate::{NodeHandle, ScriptContext, ScriptError, ScriptResult};
use glam::Vec3;
use raf_core::scene::joint::{JointKind, JointMotor, PhysicsJoint};

const JOINTS_PER_ENTITY: usize = 32;

fn invalid(message: &str) -> ScriptError {
    ScriptError::InvalidArgument(message.into())
}

impl NodeHandle {
    pub fn attach_collider_to_parent(
        &self,
        ctx: &mut ScriptContext<'_>,
        attached: bool,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let node = ctx
            .scene
            .get(id)
            .ok_or_else(|| invalid("Collider was deleted"))?;
        let owner = if attached {
            if node.rigid_body.enabled {
                return Err(invalid(
                    "A compound collider cannot have its own active body",
                ));
            }
            let parent = node
                .parent
                .and_then(|id| ctx.scene.get(id))
                .filter(|node| node.rigid_body.enabled)
                .ok_or_else(|| invalid("An enabled direct parent body is required"))?;
            Some(parent.uuid)
        } else {
            None
        };
        ctx.scene
            .get_mut(id)
            .ok_or_else(|| invalid("Collider was deleted"))?
            .collider
            .body_owner = owner;
        Ok(())
    }
    pub fn set_collision_filter(
        &self,
        ctx: &mut ScriptContext<'_>,
        layer: u32,
        mask: u32,
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let collider = &mut ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| invalid("Collider was deleted"))?
            .collider;
        collider.collision_layer = layer;
        collider.collision_mask = mask;
        Ok(())
    }

    pub fn add_physics_joint(
        &self,
        ctx: &mut ScriptContext<'_>,
        target: NodeHandle,
        kind: JointKind,
        anchors: [Vec3; 2],
        axes: [Vec3; 2],
    ) -> ScriptResult<usize> {
        let id = self.resolve(ctx)?;
        let target_id = target.resolve(ctx)?;
        if id == target_id {
            return Err(invalid("Cannot constrain an entity to itself"));
        }
        let target = ctx
            .scene
            .get(target_id)
            .ok_or_else(|| invalid("Joint target was deleted"))?
            .uuid;
        let mut joint = PhysicsJoint::new(target, kind, anchors[0], anchors[1]);
        joint.local_axis = axes[0];
        joint.target_axis = axes[1];
        if !joint.is_valid() {
            return Err(invalid("Invalid physics joint anchors or axes"));
        }
        let joints = &mut ctx
            .scene
            .get_mut(id)
            .ok_or_else(|| invalid("Joint owner was deleted"))?
            .physics_joints;
        if joints.len() >= JOINTS_PER_ENTITY {
            return Err(invalid("Joint count limit exceeded"));
        }
        let index = joints.len();
        joints.push(joint);
        Ok(index)
    }

    pub fn configure_joint_motion(
        &self,
        ctx: &mut ScriptContext<'_>,
        index: usize,
        limits: Option<[f32; 2]>,
        motor: Option<JointMotor>,
        break_impulse: Option<f32>,
    ) -> ScriptResult<()> {
        self.edit_joint(ctx, index, |joint| {
            joint.limits = limits;
            joint.motor = motor;
            joint.break_impulse = break_impulse;
        })
    }

    pub fn configure_joint_spring(
        &self,
        ctx: &mut ScriptContext<'_>,
        index: usize,
        rest_length: f32,
        stiffness: f32,
        damping: f32,
    ) -> ScriptResult<()> {
        self.edit_joint(ctx, index, |joint| {
            joint.rest_length = rest_length;
            joint.stiffness = stiffness;
            joint.damping = damping;
        })
    }

    fn edit_joint(
        &self,
        ctx: &mut ScriptContext<'_>,
        index: usize,
        mutate: impl FnOnce(&mut PhysicsJoint),
    ) -> ScriptResult<()> {
        let id = self.resolve(ctx)?;
        let joint = ctx
            .scene
            .get_mut(id)
            .and_then(|node| node.physics_joints.get_mut(index))
            .ok_or_else(|| invalid("Invalid joint index"))?;
        let mut candidate = joint.clone();
        mutate(&mut candidate);
        if !candidate.is_valid() {
            return Err(invalid("Invalid joint settings"));
        }
        *joint = candidate;
        Ok(())
    }
}
