//! An animation-driven torque controller with physical override per joint.

use glam::{Mat4, Quat};
use raf_core::scene::animation::{degrees, resolve_path, PhysicsFollower};
use raf_core::scene::{RigidBodyType, SceneGraph, SceneNodeId};
use uuid::Uuid;

pub(super) struct BoundFollower {
    joint: (SceneNodeId, Uuid),
    body: SceneNodeId,
    settings: PhysicsFollower,
    goal: Option<Quat>,
}

impl BoundFollower {
    pub(super) fn bind(
        scene: &SceneGraph,
        owner: SceneNodeId,
        settings: &PhysicsFollower,
    ) -> Result<Self, String> {
        let id = resolve_path(scene, owner, &settings.path)?;
        let joint = scene.get(id).ok_or("Physics follower joint was removed")?;
        let body = scene
            .iter()
            .find(|(_, node)| {
                !node.name.is_empty()
                    && node.uuid == settings.body
                    && node.rigid_body.enabled
                    && node.rigid_body.body_type == RigidBodyType::Dynamic
                    && !node.rigid_body.is_character
            })
            .map(|(id, _)| id)
            .ok_or("Physics follower needs an enabled dynamic body")?;
        Ok(Self {
            joint: (id, joint.uuid),
            body,
            settings: settings.clone(),
            goal: None,
        })
    }

    pub(super) fn sample_goal(&mut self, scene: &SceneGraph) {
        self.goal = scene
            .get(self.joint.0)
            .filter(|node| !node.name.is_empty() && node.uuid == self.joint.1)
            .and_then(|_| rotation(scene.world_matrix(self.joint.0)));
    }

    pub(super) fn drive(&self, scene: &mut SceneGraph) {
        let Some(goal) = self.goal else {
            return;
        };
        let Some(current) = rotation(scene.world_matrix(self.body)) else {
            return;
        };
        let Some(node) = scene.get(self.body).filter(|node| {
            node.uuid == self.settings.body
                && !node.name.is_empty()
                && node.rigid_body.enabled
                && node.rigid_body.body_type == RigidBodyType::Dynamic
        }) else {
            return;
        };
        let error = goal * current.inverse();
        let (axis, angle) = (if error.w < 0.0 { -error } else { error }).to_axis_angle();
        let torque = (axis * angle * self.settings.stiffness
            - node.rigid_body.angular_velocity * self.settings.damping)
            .clamp_length_max(self.settings.max_torque);
        if torque.is_finite() && torque != glam::Vec3::ZERO {
            scene
                .get_mut(self.body)
                .expect("bound body")
                .rigid_body
                .torque += torque;
        }
    }

    pub(super) fn apply(&self, scene: &mut SceneGraph) {
        if self.settings.weight == 0.0 || self.goal.is_none() {
            return;
        }
        let Some(body) = scene
            .get(self.body)
            .filter(|node| node.uuid == self.settings.body && !node.name.is_empty())
        else {
            return;
        };
        if !body.rigid_body.enabled || body.rigid_body.body_type != RigidBodyType::Dynamic {
            return;
        }
        let Some(joint) = scene.get(self.joint.0) else {
            return;
        };
        let parent = joint
            .parent
            .map(|id| scene.world_matrix(id))
            .unwrap_or(Mat4::IDENTITY);
        let pose = parent.inverse() * scene.world_matrix(self.body);
        let Some(rotation) = rotation(pose) else {
            return;
        };
        let position = pose.w_axis.truncate();
        if !position.is_finite() {
            return;
        }
        let node = scene.get_mut(self.joint.0).expect("bound joint");
        node.rotation = degrees(
            raf_core::scene::animation::to_quat(node.rotation)
                .slerp(rotation, self.settings.weight),
        );
        node.position = node.position.lerp(position, self.settings.weight);
    }
}

fn rotation(pose: Mat4) -> Option<Quat> {
    if !pose.is_finite() || pose.determinant().abs() < 1e-8 {
        return None;
    }
    let (_, rotation, _) = pose.to_scale_rotation_translation();
    rotation.is_finite().then(|| rotation.normalize())
}
