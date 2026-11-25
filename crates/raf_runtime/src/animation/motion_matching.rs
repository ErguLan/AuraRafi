//! Bounded motion matching over authored pose samples and planar trajectories.

use super::{Rig, Target};
use glam::{Quat, Vec3};
use raf_core::scene::animation::{
    degrees, to_quat, AnimationChannel, AnimationLayer, MotionMatching,
};

const FEATURE_HZ: f32 = 12.0;
const SEARCH_INTERVAL: f32 = 1.0 / FEATURE_HZ;
const BLEND_TIME: f32 = 0.12;
const MAX_SAMPLES: usize = 4096;

#[derive(Clone, Copy)]
struct Playback {
    clip: usize,
    time: f32,
}

struct Feature {
    playback: Playback,
    pose: Vec3,
    displacement: Vec3,
}

pub(super) struct MotionSearch {
    features: Vec<Feature>,
    pose_tracks: Vec<usize>,
    lookahead: f32,
    pose_weight: f32,
    timer: f32,
    current: Option<Playback>,
    previous: Option<Playback>,
    blend: f32,
}

impl MotionSearch {
    pub(super) fn prepare(
        layers: &[AnimationLayer],
        profile: &MotionMatching,
    ) -> Result<Self, String> {
        let mut features = Vec::new();
        let mut pose_tracks = Vec::new();
        for (clip, layer) in layers.iter().enumerate() {
            let index = layer
                .tracks
                .iter()
                .position(|track| {
                    track.channel == AnimationChannel::Position && track.path == profile.pose_path
                })
                .ok_or("Missing motion feature track")?;
            pose_tracks.push(index);
            let samples = (layer.duration * FEATURE_HZ).ceil().max(1.0) as usize;
            if samples > MAX_SAMPLES - features.len() {
                return Err("Motion matching exceeds its sample budget".into());
            }
            let direction =
                Quat::from_rotation_y(layer.motion_heading_degrees.to_radians()) * -Vec3::Z;
            let displacement = direction * layer.motion_speed.unwrap_or(0.0) * profile.lookahead;
            for sample in 0..samples {
                let time = sample as f32 * layer.duration / samples as f32;
                let pose = layer.tracks[index].sample(time);
                features.push(Feature {
                    playback: Playback { clip, time },
                    pose,
                    displacement,
                });
            }
        }
        Ok(Self {
            features,
            pose_tracks,
            lookahead: profile.lookahead,
            pose_weight: profile.pose_weight,
            timer: 0.0,
            current: None,
            previous: None,
            blend: 0.0,
        })
    }

    pub(super) fn step(&mut self, layers: &[AnimationLayer], wanted: Vec3, dt: f32) {
        let advance = |play: &mut Playback| {
            play.time = (play.time + dt).rem_euclid(layers[play.clip].duration)
        };
        if let Some(active) = &mut self.current {
            advance(active);
        }
        if let Some(previous) = &mut self.previous {
            advance(previous);
        }
        self.blend = (self.blend + dt / BLEND_TIME).min(1.0);
        if self.blend >= 1.0 {
            self.previous = None;
        }
        self.timer -= dt;
        if self.timer > 0.0 {
            return;
        }
        self.timer = SEARCH_INTERVAL;
        let pose = self.current.map_or_else(
            || self.features[0].pose,
            |play| layers[play.clip].tracks[self.pose_tracks[play.clip]].sample(play.time),
        );
        let candidate = self.best(pose, wanted);
        if self.current.is_some_and(|active| active.clip == candidate.clip) {
            return;
        }
        self.previous = self.current;
        self.current = Some(candidate);
        self.blend = if self.previous.is_some() { 0.0 } else { 1.0 };
    }

    fn best(&self, pose: Vec3, wanted: Vec3) -> Playback {
        let displacement = wanted * self.lookahead;
        self.features
            .iter()
            .min_by(|a, b| {
                let cost = |feature: &Feature| {
                    feature.displacement.distance_squared(displacement)
                        + self.pose_weight * feature.pose.distance_squared(pose)
                };
                cost(a).total_cmp(&cost(b))
            })
            .expect("validated matching profile has samples")
            .playback
    }
}

pub(super) fn apply(
    rig: &Rig,
    search: &MotionSearch,
    seconds: f64,
    speed: f32,
    targets: &mut [Target],
) {
    let current = search.current.unwrap_or_else(|| {
        let mut choice = search.best(search.features[0].pose, -Vec3::Z * speed);
        choice.time = (f64::from(choice.time) + seconds)
            .rem_euclid(f64::from(rig.authored.layers[choice.clip].duration))
            as f32;
        choice
    });
    let previous = search.previous.unwrap_or(current);
    let mix = if search.previous.is_some() {
        search.blend
    } else {
        1.0
    };
    let active = &rig.authored.layers[current.clip];
    let old = &rig.authored.layers[previous.clip];
    for ((a, b), &index) in old
        .tracks
        .iter()
        .zip(&active.tracks)
        .zip(&rig.bindings[current.clip])
    {
        let pose = &mut targets[index].pose;
        let end = b.sample(current.time);
        let start = if mix == 1.0 {
            end
        } else {
            a.sample(previous.time)
        };
        match b.channel {
            AnimationChannel::Position => pose.position = start.lerp(end, mix),
            AnimationChannel::Rotation => {
                pose.rotation = degrees(to_quat(start).slerp(to_quat(end), mix))
            }
            AnimationChannel::Scale => pose.scale = start.lerp(end, mix),
        }
    }
}
