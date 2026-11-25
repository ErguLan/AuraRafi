//! Authored transform rigs. Track targets are unambiguous child paths within the rig.

use super::{SceneGraph, SceneNodeId};
use glam::{EulerRot, Quat, Vec3};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

const MAX_LAYERS: usize = 32;
const MAX_TRACKS: usize = 512;
const MAX_KEYS: usize = 32_768;
const MAX_SECONDS: f32 = 3_600.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationRig {
    pub layers: Vec<AnimationLayer>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impact: Option<ImpactReaction>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contacts: Vec<PoseContact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_match: Option<MotionMatching>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub physics_follow: Vec<PhysicsFollower>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PhysicsFollower {
    pub path: Vec<String>,
    pub body: uuid::Uuid,
    pub weight: f32,
    pub stiffness: f32,
    pub damping: f32,
    pub max_torque: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionMatching {
    pub pose_path: Vec<String>,
    pub lookahead: f32,
    pub pose_weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoseContact {
    /// Direct root, middle and tip chain, addressed relative to the rig owner.
    pub chain: [Vec<String>; 3],
    pub goal: ContactGoal,
    /// Bend hint relative to the rig owner.
    pub pole: Vec3,
    /// Half-extent of the conservative tip sweep, in world meters.
    pub radius: f32,
    #[serde(default = "one")]
    pub weight: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactGoal {
    Ground {
        above: f32,
        below: f32,
        minimum_alignment: f32,
    },
    Target {
        entity: uuid::Uuid,
        offset: Vec3,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationLayer {
    pub name: String,
    pub duration: f32,
    #[serde(default)]
    pub looping: bool,
    #[serde(default = "one")]
    pub weight: f32,
    #[serde(default)]
    pub start: f32,
    #[serde(default)]
    pub fade_in: f32,
    /// Optional fade to zero before the end of a non-looping layer.
    #[serde(default)]
    pub fade_out: f32,
    /// Base locomotion sample in m/s. Speed layers share one normalized gait cycle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motion_speed: Option<f32>,
    #[serde(default)]
    pub motion_heading_degrees: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub action: Option<ActionLayer>,
    /// Add translations and post-multiply rotations instead of overriding.
    #[serde(default)]
    pub additive: bool,
    pub tracks: Vec<AnimationTrack>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActionLayer {
    /// Matched against the owner's `animation_action` text variable.
    pub request: String,
    /// Entity-reference variables that must exist and be empty to use this layer.
    #[serde(default)]
    pub free_slots: Vec<String>,
    #[serde(default)]
    pub priority: i16,
    #[serde(default)]
    pub interruptible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationTrack {
    /// Empty resolves to the rig owner. Each segment must name exactly one child.
    #[serde(default)]
    pub path: Vec<String>,
    pub channel: AnimationChannel,
    #[serde(default)]
    pub stepped: bool,
    pub keys: Vec<AnimationKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnimationChannel {
    Position,
    Rotation,
    Scale,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnimationKey {
    pub time: f32,
    /// Local meters, Euler degrees (YXZ), or scale, depending on channel.
    pub value: Vec3,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImpactReaction {
    pub path: Vec<String>,
    pub duration: f32,
    pub meters_per_impulse: f32,
    pub max_offset: f32,
}

fn one() -> f32 {
    1.0
}

impl AnimationRig {
    pub fn validate(&self) -> Result<(), String> {
        if self.layers.len() > MAX_LAYERS
            || self.contacts.len() > 16
            || self.physics_follow.len() > 64
            || (self.layers.is_empty()
                && self.impact.is_none()
                && self.contacts.is_empty()
                && self.physics_follow.is_empty())
        {
            return Err("Animation rig has an invalid layer count".into());
        }
        let mut joints = HashSet::new();
        for contact in &self.contacts {
            for path in &contact.chain {
                validate_path(path)?;
                if !joints.insert(path) {
                    return Err("Contact chains cannot share joints".into());
                }
            }
            if !contact.pole.is_finite()
                || !(0.005..=1.0).contains(&contact.radius)
                || !(0.0..=1.0).contains(&contact.weight)
            {
                return Err("Invalid contact chain settings".into());
            }
            match contact.goal {
                ContactGoal::Ground {
                    above,
                    below,
                    minimum_alignment,
                } if (0.0..=2.0).contains(&above)
                    && (0.005..=3.0).contains(&below)
                    && (0.01..=1.0).contains(&minimum_alignment) => {}
                ContactGoal::Target { offset, .. } if offset.is_finite() => {}
                _ => return Err("Invalid animation contact goal".into()),
            }
        }
        let mut followers = HashSet::new();
        let mut bodies = HashSet::new();
        for follower in &self.physics_follow {
            validate_path(&follower.path)?;
            if !followers.insert(&follower.path)
                || !bodies.insert(follower.body)
                || !(0.0..=1.0).contains(&follower.weight)
                || !(0.0..=10_000.0).contains(&follower.stiffness)
                || !(0.0..=1_000.0).contains(&follower.damping)
                || !(0.0..=100_000.0).contains(&follower.max_torque)
            {
                return Err("Invalid or duplicate physics follower".into());
            }
        }
        let mut names = HashSet::new();
        let mut tracks = 0;
        let mut keys = 0;
        let mut previous_motion: Option<(&AnimationLayer, f32)> = None;
        let mut overlays_started = false;
        for layer in &self.layers {
            tracks += layer.tracks.len();
            if tracks > MAX_TRACKS
                || layer.tracks.is_empty()
                || layer.name.trim().is_empty()
                || layer.name.len() > 128
                || !names.insert(&layer.name)
                || !valid_duration(layer.duration)
                || !(0.0..=1.0).contains(&layer.weight)
                || !(-360.0..=360.0).contains(&layer.motion_heading_degrees)
                || !(0.0..=MAX_SECONDS).contains(&layer.start)
                || !(0.0..=layer.duration).contains(&layer.fade_in)
                || !(0.0..=layer.duration).contains(&layer.fade_out)
                || (layer.looping && layer.fade_out != 0.0)
            {
                return Err(format!("Invalid animation layer {:?}", layer.name));
            }
            if let Some(action) = &layer.action {
                if action.request.trim().is_empty()
                    || action.request.len() > 128
                    || action.free_slots.len() > 8
                    || action
                        .free_slots
                        .iter()
                        .any(|slot| slot.trim().is_empty() || slot.len() > 128)
                    || layer.looping
                    || layer.start != 0.0
                    || layer.motion_speed.is_some()
                {
                    return Err(format!("Invalid animation action {:?}", action.request));
                }
            }
            if let Some(speed) = layer.motion_speed {
                if overlays_started
                    || !(0.0..=1_000.0).contains(&speed)
                    || !layer.looping
                    || layer.additive
                    || layer.weight != 1.0
                    || layer.start != 0.0
                    || layer.fade_in != 0.0
                    || layer.fade_out != 0.0
                    || previous_motion.is_some_and(|(prior, prior_speed)| {
                        (self.motion_match.is_none()
                            && (speed <= prior_speed || prior.duration != layer.duration))
                            || prior.tracks.len() != layer.tracks.len()
                            || prior
                                .tracks
                                .iter()
                                .zip(&layer.tracks)
                                .any(|(a, b)| a.path != b.path || a.channel != b.channel)
                    })
                {
                    return Err("Motion layers must come first, use increasing speeds, and share the same loop duration and track bindings".into());
                }
                previous_motion = Some((layer, speed));
            } else {
                overlays_started = true;
            }
            let mut targets = HashSet::new();
            for track in &layer.tracks {
                keys += track.keys.len();
                if keys > MAX_KEYS || !targets.insert((&track.path, track.channel)) {
                    return Err(format!("Invalid or duplicate track in {:?}", layer.name));
                }
                validate_path(&track.path)?;
                let mut previous = -1.0;
                if track.keys.is_empty() {
                    return Err("Animation track has no keys".into());
                }
                for key in &track.keys {
                    if !(0.0..=layer.duration).contains(&key.time)
                        || key.time <= previous
                        || !key.value.is_finite()
                        || key.value.abs().max_element() > 1_000_000.0
                        || (track.channel == AnimationChannel::Scale
                            && key.value.min_element() <= 0.0)
                    {
                        return Err(format!("Invalid animation key in {:?}", layer.name));
                    }
                    previous = key.time;
                }
            }
        }
        if let Some(settings) = &self.motion_match {
            validate_path(&settings.pose_path)?;
            if !(0.05..=2.0).contains(&settings.lookahead)
                || !(0.0..=100.0).contains(&settings.pose_weight)
                || previous_motion.is_none()
                || self
                    .layers
                    .iter()
                    .take_while(|layer| layer.motion_speed.is_some())
                    .any(|layer| {
                        !layer.tracks.iter().any(|track| {
                            track.path == settings.pose_path
                                && track.channel == AnimationChannel::Position
                        })
                    })
            {
                return Err(
                    "Motion matching needs a position feature on every locomotion clip".into(),
                );
            }
        }
        if let Some(reaction) = &self.impact {
            validate_path(&reaction.path)?;
            if !valid_duration(reaction.duration)
                || !(0.0..=1.0).contains(&reaction.meters_per_impulse)
                || !(0.0..=10.0).contains(&reaction.max_offset)
            {
                return Err("Invalid animation impact response".into());
            }
        }
        Ok(())
    }
}

fn valid_duration(duration: f32) -> bool {
    (0.0..=MAX_SECONDS).contains(&duration) && duration > 0.0
}

fn validate_path(path: &[String]) -> Result<(), String> {
    if path.len() > 64
        || path
            .iter()
            .any(|segment| segment.trim().is_empty() || segment.len() > 255)
    {
        return Err("Invalid animation target path".into());
    }
    Ok(())
}

pub fn resolve_path(
    scene: &SceneGraph,
    owner: SceneNodeId,
    path: &[String],
) -> Result<SceneNodeId, String> {
    let mut target = owner;
    for segment in path {
        let mut matches = scene
            .get(target)
            .into_iter()
            .flat_map(|node| &node.children)
            .filter(|&&id| {
                scene
                    .get(id)
                    .is_some_and(|child| child.name == *segment && child.parent == Some(target))
            });
        target = *matches
            .next()
            .filter(|_| matches.next().is_none())
            .ok_or_else(|| format!("Animation target is missing or ambiguous at {segment:?}"))?;
    }
    Ok(target)
}

impl AnimationLayer {
    pub fn sample_at(&self, seconds: f64) -> Option<(f32, f32)> {
        let duration = f64::from(self.duration);
        let elapsed = seconds - f64::from(self.start);
        if elapsed < 0.0 || self.weight == 0.0 {
            return None;
        }
        let appear = if self.fade_in == 0.0 {
            1.0
        } else {
            (elapsed / f64::from(self.fade_in)).min(1.0) as f32
        };
        let disappear = if self.fade_out == 0.0 {
            1.0
        } else {
            ((duration - elapsed) / f64::from(self.fade_out)).clamp(0.0, 1.0) as f32
        };
        let weight = self.weight * appear.min(disappear);
        if weight == 0.0 {
            return None;
        }
        let time = if self.looping {
            elapsed.rem_euclid(duration) as f32
        } else {
            elapsed.min(duration) as f32
        };
        Some((time, weight))
    }
}

impl AnimationTrack {
    pub fn sample(&self, time: f32) -> Vec3 {
        let right = self.keys.partition_point(|key| key.time <= time);
        let before = self.keys[right.saturating_sub(1)];
        let Some(after) = self.keys.get(right).filter(|_| right > 0 && !self.stepped) else {
            return before.value;
        };
        let fraction = (time - before.time) / (after.time - before.time);
        match self.channel {
            AnimationChannel::Rotation => {
                degrees(to_quat(before.value).slerp(to_quat(after.value), fraction))
            }
            _ => before.value.lerp(after.value, fraction),
        }
    }
}

pub fn to_quat(value: Vec3) -> Quat {
    Quat::from_euler(
        EulerRot::YXZ,
        value.y.to_radians(),
        value.x.to_radians(),
        value.z.to_radians(),
    )
}

pub fn degrees(rotation: Quat) -> Vec3 {
    let (y, x, z) = rotation.to_euler(EulerRot::YXZ);
    Vec3::new(x.to_degrees(), y.to_degrees(), z.to_degrees())
}
