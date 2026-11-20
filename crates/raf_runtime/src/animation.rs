//! Fixed-step transform animation and read-only authoring evaluation.

use crate::physics::PhysicsImpact;
use glam::{Mat4, Quat, Vec3};
use raf_core::scene::animation::{
    degrees, resolve_path, to_quat, AnimationChannel, AnimationRig, ImpactReaction,
};
use raf_core::scene::{ColliderType, SceneGraph, SceneNode, SceneNodeId, WorldTransformCache};
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

#[cfg(test)]
mod tests;

const MAX_SCENE_TRACKS: usize = 32_768;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct TransformChannels {
    pub position: Vec3,
    pub rotation: Vec3,
    pub scale: Vec3,
}

impl From<&SceneNode> for TransformChannels {
    fn from(node: &SceneNode) -> Self {
        Self {
            position: node.position,
            rotation: node.rotation,
            scale: node.scale,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct AnimationPose {
    pub node: SceneNodeId,
    pub rig: SceneNodeId,
    pub transform: TransformChannels,
}

struct Target {
    node: SceneNodeId,
    owner: SceneNodeId,
    uuid: Uuid,
    rest: TransformChannels,
    pose: TransformChannels,
    position: bool,
    rotation: bool,
    scale: bool,
}

struct Reaction {
    target: usize,
    source: Uuid,
    settings: ImpactReaction,
    displacement: Vec3,
    magnitude: f32,
    remaining: f32,
}

struct Rig {
    owner: SceneNodeId,
    motion_layers: usize,
    authored: AnimationRig,
    bindings: Vec<Vec<usize>>,
    reaction: Option<Reaction>,
}

#[derive(Default)]
pub struct AnimationPlayback {
    rigs: Vec<Rig>,
    targets: Vec<Target>,
    bones: Vec<(SceneNodeId, Uuid)>,
}

impl AnimationPlayback {
    pub fn prepare(scene: &SceneGraph) -> Result<Self, String> {
        let mut playback = Self::default();
        let mut owned = HashSet::new();
        let mut protected = HashSet::new();
        let mut track_count = 0;
        let mut rigs = scene
            .iter()
            .filter(|(_, node)| !node.name.is_empty())
            .filter_map(|(id, node)| node.animation.as_ref().map(|rig| (id, node, rig)))
            .peekable();
        if rigs.peek().is_none() {
            return Ok(playback);
        }
        for (id, node) in scene.iter() {
            if node.rigid_body.enabled || node.collider.collider_type != ColliderType::None {
                let mut current = Some(id);
                while let Some(id) = current.filter(|id| protected.insert(*id)) {
                    current = scene.get(id).and_then(|node| node.parent);
                }
            }
        }
        for (owner, node, rig) in rigs {
            rig.validate()
                .map_err(|err| format!("{}: {err}", node.name))?;
            let mut local = HashMap::new();
            let mut bind = |path: &[String], channel: AnimationChannel| -> Result<usize, String> {
                let id = resolve_path(scene, owner, path)?;
                let target = scene.get(id).ok_or("Missing animation target")?;
                if protected.contains(&id) {
                    return Err(format!(
                        "Animation target {:?} moves a physics body or collider",
                        target.name
                    ));
                }
                let index = if let Some(&index) = local.get(&id) {
                    index
                } else {
                    if !owned.insert(id) {
                        return Err(format!(
                            "Animation target {:?} belongs to multiple rigs",
                            target.name
                        ));
                    }
                    let index = playback.targets.len();
                    let rest = TransformChannels::from(target);
                    playback.targets.push(Target {
                        node: id,
                        owner,
                        uuid: target.uuid,
                        rest,
                        pose: rest,
                        position: false,
                        rotation: false,
                        scale: false,
                    });
                    local.insert(id, index);
                    index
                };
                let target = &mut playback.targets[index];
                match channel {
                    AnimationChannel::Position => target.position = true,
                    AnimationChannel::Rotation => target.rotation = true,
                    AnimationChannel::Scale => target.scale = true,
                }
                Ok(index)
            };
            let mut bindings = Vec::with_capacity(rig.layers.len());
            for layer in &rig.layers {
                track_count += layer.tracks.len();
                if track_count > MAX_SCENE_TRACKS {
                    return Err("Scene exceeds the animation track budget".into());
                }
                bindings.push(
                    layer
                        .tracks
                        .iter()
                        .map(|track| bind(&track.path, track.channel))
                        .collect::<Result<Vec<_>, _>>()?,
                );
            }
            let reaction = rig
                .impact
                .as_ref()
                .map(|settings| {
                    Ok::<_, String>(Reaction {
                        target: bind(&settings.path, AnimationChannel::Position)?,
                        source: node.uuid,
                        settings: settings.clone(),
                        displacement: Vec3::ZERO,
                        magnitude: 0.0,
                        remaining: 0.0,
                    })
                })
                .transpose()?;
            playback.rigs.push(Rig {
                owner,
                motion_layers: rig
                    .layers
                    .iter()
                    .take_while(|layer| layer.motion_speed.is_some())
                    .count(),
                authored: rig.clone(),
                bindings,
                reaction,
            });
        }
        let mut bones = HashSet::new();
        for target in &playback.targets {
            let mut cursor = Some(target.node);
            while let Some(id) = cursor.filter(|id| bones.insert(*id)) {
                let Some(node) = scene.get(id) else { break };
                if id == target.owner {
                    break;
                }
                playback.bones.push((id, node.uuid));
                cursor = node.parent;
            }
        }
        Ok(playback)
    }

    pub fn bone_nodes(&self) -> &[(SceneNodeId, Uuid)] {
        &self.bones
    }

    pub fn sample(
        &mut self,
        seconds: f64,
        speed: f32,
    ) -> Result<impl Iterator<Item = AnimationPose> + '_, String> {
        if !seconds.is_finite() || seconds < 0.0 || !speed.is_finite() || speed < 0.0 {
            return Err("Animation sample time and speed must be finite and nonnegative".into());
        }
        self.evaluate(seconds, |_| speed);
        Ok(self.targets.iter().map(|target| AnimationPose {
            node: target.node,
            rig: target.owner,
            transform: target.pose,
        }))
    }

    fn evaluate(&mut self, seconds: f64, speed: impl Fn(SceneNodeId) -> f32) {
        for target in &mut self.targets {
            target.pose = target.rest;
        }
        for rig in &self.rigs {
            if rig.motion_layers != 0 {
                sample_motion(rig, seconds, speed(rig.owner), &mut self.targets);
            }
            for (layer, bindings) in rig
                .authored
                .layers
                .iter()
                .zip(&rig.bindings)
                .skip(rig.motion_layers)
            {
                let Some((time, weight)) = layer.sample_at(seconds) else {
                    continue;
                };
                for (track, &idx) in layer.tracks.iter().zip(bindings) {
                    let pose = &mut self.targets[idx].pose;
                    let value = track.sample(time);
                    match (track.channel, layer.additive) {
                        (AnimationChannel::Position, false) => {
                            pose.position = pose.position.lerp(value, weight)
                        }
                        (AnimationChannel::Position, true) => pose.position += value * weight,
                        (AnimationChannel::Rotation, false) => {
                            pose.rotation =
                                degrees(to_quat(pose.rotation).slerp(to_quat(value), weight))
                        }
                        (AnimationChannel::Rotation, true) => {
                            pose.rotation = degrees(
                                to_quat(pose.rotation)
                                    * Quat::IDENTITY.slerp(to_quat(value), weight),
                            )
                        }
                        (AnimationChannel::Scale, false) => {
                            pose.scale = pose.scale.lerp(value, weight)
                        }
                        (AnimationChannel::Scale, true) => {
                            pose.scale *= Vec3::ONE.lerp(value, weight)
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn tick(
        &mut self,
        scene: &mut SceneGraph,
        time: f64,
        dt: f32,
        impacts: &[PhysicsImpact],
        gravity: Option<Vec3>,
    ) {
        if self.rigs.is_empty() {
            return;
        }
        let up = gravity
            .and_then(|g| (-g).try_normalize())
            .unwrap_or(Vec3::Y);
        self.evaluate(time, |id| {
            let velocity = scene
                .get(id)
                .filter(|_| gravity.is_some())
                .filter(|node| {
                    node.rigid_body.enabled
                        && node.rigid_body.body_type != raf_core::scene::RigidBodyType::Static
                })
                .map(|node| node.rigid_body.velocity)
                .filter(|v| v.is_finite())
                .unwrap_or(Vec3::ZERO);
            (velocity - up * velocity.dot(up)).length()
        });
        let mut world_transforms = None;
        for rig in &mut self.rigs {
            let Some(reaction) = &mut rig.reaction else {
                continue;
            };
            reaction.remaining = (reaction.remaining - dt).max(0.0);
            let strongest = impacts
                .iter()
                .filter_map(|hit| {
                    let sign = if hit.body == reaction.source {
                        1.0
                    } else if hit.other == reaction.source {
                        -1.0
                    } else {
                        return None;
                    };
                    Some((hit.normal * sign, hit.normal_impulse))
                })
                .filter(|(_, impulse)| impulse.is_finite() && *impulse > 0.0)
                .max_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((normal, impulse)) = strongest {
                let magnitude = (impulse * reaction.settings.meters_per_impulse)
                    .min(reaction.settings.max_offset);
                let active =
                    reaction.magnitude * (reaction.remaining / reaction.settings.duration).powi(2);
                let target = self.targets[reaction.target].node;
                if active < magnitude && scene.is_valid_node(target) {
                    let parent = scene.get(target).and_then(|node| node.parent);
                    let transform = parent
                        .and_then(|id| {
                            world_transforms
                                .get_or_insert_with(|| WorldTransformCache::build(scene))
                                .world_matrix(id)
                        })
                        .unwrap_or(Mat4::IDENTITY);
                    let displacement = transform.inverse().transform_vector3(normal * magnitude);
                    if displacement.is_finite() {
                        reaction.displacement = displacement;
                        reaction.magnitude = magnitude;
                        reaction.remaining = reaction.settings.duration;
                    }
                }
            }
            self.targets[reaction.target].pose.position +=
                reaction.displacement * (reaction.remaining / reaction.settings.duration).powi(2);
        }
        for target in &self.targets {
            let Some(current) = scene.get(target.node).filter(|node| {
                node.uuid == target.uuid
                    && !node.name.is_empty()
                    && !node.rigid_body.enabled
                    && node.collider.collider_type == ColliderType::None
            }) else {
                continue;
            };
            let pose = target.pose;
            let position = if target.position {
                pose.position
            } else {
                current.position
            };
            let rotation = if target.rotation {
                pose.rotation
            } else {
                current.rotation
            };
            let scale = if target.scale {
                pose.scale
            } else {
                current.scale
            };
            if position.is_finite()
                && rotation.is_finite()
                && scale.is_finite()
                && [position, rotation, scale]
                    != [current.position, current.rotation, current.scale]
            {
                if let Some(node) = scene.get_mut(target.node) {
                    [node.position, node.rotation, node.scale] = [position, rotation, scale];
                }
            }
        }
    }
}

fn sample_motion(rig: &Rig, seconds: f64, speed: f32, targets: &mut [Target]) {
    let layers = &rig.authored.layers[..rig.motion_layers];
    let split = layers.partition_point(|layer| layer.motion_speed.unwrap_or(0.0) <= speed);
    let lower = split.saturating_sub(1);
    let upper = split.min(layers.len() - 1);
    let a = &layers[lower];
    let b = &layers[upper];
    let t = if lower == upper {
        0.0
    } else {
        let min = a.motion_speed.unwrap_or(0.0);
        let max = b.motion_speed.unwrap_or(0.0);
        ((speed - min) / (max - min)).clamp(0.0, 1.0)
    };
    let time = seconds.rem_euclid(f64::from(a.duration)) as f32;
    for ((a, b), &index) in a.tracks.iter().zip(&b.tracks).zip(&rig.bindings[lower]) {
        let pose = &mut targets[index].pose;
        let start = a.sample(time);
        let end = if t == 0.0 { start } else { b.sample(time) };
        match a.channel {
            AnimationChannel::Position => pose.position = start.lerp(end, t),
            AnimationChannel::Rotation => {
                pose.rotation = degrees(to_quat(start).slerp(to_quat(end), t))
            }
            AnimationChannel::Scale => pose.scale = start.lerp(end, t),
        }
    }
}
