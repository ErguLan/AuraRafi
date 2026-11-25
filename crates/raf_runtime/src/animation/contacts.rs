//! Two-bone contact placement constrained by the physics spatial index.

use crate::physics::PhysicsWorld;
use glam::{Mat4, Quat, Vec3};
use raf_core::scene::animation::{degrees, resolve_path, ContactGoal, PoseContact};
use raf_core::scene::{IkChain, SceneGraph, SceneNodeId};
use std::collections::HashSet;
use uuid::Uuid;

pub(super) struct BoundContact {
    owner: SceneNodeId,
    chain: [(SceneNodeId, Uuid); 3],
    target: Option<(SceneNodeId, Uuid)>,
    profile: PoseContact,
    previous: Option<Vec3>,
}

impl BoundContact {
    pub(super) fn bind(
        scene: &SceneGraph,
        owner: SceneNodeId,
        profile: &PoseContact,
        protected: &HashSet<SceneNodeId>,
    ) -> Result<Self, String> {
        let mut chain = [(owner, Uuid::nil()); 3];
        for (entry, path) in chain.iter_mut().zip(&profile.chain) {
            let id = resolve_path(scene, owner, path)?;
            let joint = scene.get(id).ok_or("Contact joint was removed")?;
            if protected.contains(&id) {
                return Err("An animation contact chain cannot drive a physics body".into());
            }
            *entry = (id, joint.uuid);
        }
        if chain
            .windows(2)
            .any(|pair| scene.get(pair[1].0).and_then(|node| node.parent) != Some(pair[0].0))
        {
            return Err("Animation contact requires a direct two-bone hierarchy".into());
        }
        let target = match profile.goal {
            ContactGoal::Target { entity, .. } => Some(
                scene
                    .iter()
                    .find(|(_, node)| !node.name.is_empty() && node.uuid == entity)
                    .map(|(id, _)| (id, entity))
                    .ok_or("Animation contact target was not found")?,
            ),
            ContactGoal::Ground { .. } => None,
        };
        Ok(Self {
            owner,
            chain,
            target,
            profile: profile.clone(),
            previous: None,
        })
    }

    pub(super) fn solve(&mut self, scene: &mut SceneGraph, physics: &mut PhysicsWorld, up: Vec3) {
        if self.profile.weight == 0.0
            || self.chain.iter().any(|&(id, uuid)| {
                !scene
                    .get(id)
                    .is_some_and(|node| node.uuid == uuid && !node.name.is_empty())
            })
        {
            return;
        }
        let [root, middle, tip] = self.chain.map(|(id, _)| id);
        let [root_pose, middle_pose, tip_pose] =
            [root, middle, tip].map(|id| scene.world_matrix(id));
        let Some(root_rotation) = rigid_rotation(root_pose) else {
            return;
        };
        let Some(middle_rotation) = rigid_rotation(middle_pose) else {
            return;
        };
        let parent = scene.get(root).and_then(|node| node.parent);
        let Some(parent_rotation) = rigid_rotation(
            parent
                .map(|id| scene.world_matrix(id))
                .unwrap_or(Mat4::IDENTITY),
        ) else {
            return;
        };
        let points = [root_pose, middle_pose, tip_pose].map(|matrix| matrix.w_axis.truncate());
        let radius = self.profile.radius;
        let target = match self.profile.goal {
            ContactGoal::Ground {
                above,
                below,
                minimum_alignment,
            } => {
                let start = points[2] + up * above;
                let delta = -up * (above + below);
                physics
                    .animation_cast(scene, self.owner, start, delta, radius)
                    .filter(|(_, normal)| normal.dot(up) >= minimum_alignment)
                    .map(|(time, _)| start + delta * (time - 0.0001).max(0.0))
            }
            ContactGoal::Target { offset, .. } => self.target.and_then(|(id, uuid)| {
                scene
                    .get(id)
                    .filter(|node| node.uuid == uuid && !node.name.is_empty())
                    .map(|_| scene.world_matrix(id).transform_point3(offset))
            }),
        };
        let mut goal = points[2].lerp(target.unwrap_or(points[2]), self.profile.weight);
        let previous = self.previous.unwrap_or(points[2]);
        let delta = goal - previous;
        if let Some((time, _)) = physics.animation_cast(scene, self.owner, previous, delta, radius) {
            goal = previous + delta * (time - 0.0001).max(0.0);
        }
        self.previous = Some(points[2]);
        if goal.abs_diff_eq(points[2], 1e-5) {
            return;
        }
        let pole = scene
            .world_matrix(self.owner)
            .transform_point3(self.profile.pole);
        let Ok(mut chain) = IkChain::new(points.to_vec()) else {
            return;
        };
        if chain.solve_two_bone(goal, pole).is_err() {
            return;
        }
        let joints = chain.joints();
        let align = |a: Vec3, b: Vec3| Quat::from_rotation_arc(a.normalize(), b.normalize());
        let correction = align(points[1] - points[0], joints[1] - joints[0]);
        let new_root = correction * root_rotation;
        let new_mid = align(correction * (points[2] - points[1]), joints[2] - joints[1])
            * correction
            * middle_rotation;
        if new_root.is_finite() && new_mid.is_finite() {
            scene.get_mut(root).expect("bound joint").rotation =
                degrees(parent_rotation.inverse() * new_root);
            scene.get_mut(middle).expect("bound joint").rotation =
                degrees(new_root.inverse() * new_mid);
            self.previous = Some(joints[2]);
        }
    }
}

fn rigid_rotation(matrix: Mat4) -> Option<Quat> {
    let (scale, rotation, _) = matrix.to_scale_rotation_translation();
    (scale.min_element() > 0.0
        && scale.max_element() - scale.min_element() < scale.max_element() * 0.001
        && rotation.is_finite())
    .then_some(rotation)
}
