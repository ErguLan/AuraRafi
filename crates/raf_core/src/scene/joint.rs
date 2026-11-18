//! Authored constraints between bodies. Anchors and axes are in local space.
use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum JointKind {
    Fixed,
    Hinge,
    Slider,
    Ball,
    Spring,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JointMotor {
    /// Target in rad/s for a hinge, or m/s for a slider.
    pub velocity: f32,
    /// Maximum torque in N m for a hinge, or force in N for a slider.
    pub max_force: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicsJoint {
    pub target: Uuid,
    pub kind: JointKind,
    pub local_anchor: Vec3,
    pub target_anchor: Vec3,
    pub local_axis: Vec3,
    pub target_axis: Vec3,
    /// Radians for a hinge, meters for a slider.
    pub limits: Option<[f32; 2]>,
    pub motor: Option<JointMotor>,
    pub rest_length: f32,
    pub stiffness: f32,
    pub damping: f32,
    /// Linear impulse threshold in N s or angular impulse threshold in N m s.
    pub break_impulse: Option<f32>,
}

impl PhysicsJoint {
    pub fn new(target: Uuid, kind: JointKind, local_anchor: Vec3, target_anchor: Vec3) -> Self {
        Self {
            target,
            kind,
            local_anchor,
            target_anchor,
            local_axis: Vec3::X,
            target_axis: Vec3::X,
            limits: None,
            motor: None,
            rest_length: 0.0,
            stiffness: 20.0,
            damping: 2.0,
            break_impulse: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.local_anchor.is_finite()
            && self.target_anchor.is_finite()
            && self.local_axis.try_normalize().is_some()
            && self.target_axis.try_normalize().is_some()
            && self
                .limits
                .is_none_or(|[min, max]| min.is_finite() && max.is_finite() && min <= max)
            && self
                .motor
                .is_none_or(|motor| motor.velocity.is_finite() && nonnegative(motor.max_force))
            && nonnegative(self.rest_length)
            && nonnegative(self.stiffness)
            && nonnegative(self.damping)
            && self
                .break_impulse
                .is_none_or(|value| value.is_finite() && value > 0.0)
    }
}

fn nonnegative(value: f32) -> bool {
    value.is_finite() && value >= 0.0
}
