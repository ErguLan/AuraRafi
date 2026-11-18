use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Opt-in capsule controller. Distances are meters and velocity is meters/second.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CharacterController {
    pub radius: f32,
    pub height: f32,
    pub max_slope_degrees: f32,
    pub max_step_height: f32,
    pub support_distance: f32,
    pub jump_height: f32,
    #[serde(skip)]
    pub state: CharacterState,
}

#[derive(Debug, Clone, Default)]
pub struct CharacterState {
    /// Desired world-space movement velocity. The vertical component is ignored.
    pub desired_velocity: Vec3,
    pub jump_requested: bool,
    pub grounded: bool,
    pub ground_normal: Vec3,
    pub up: Vec3,
    pub support_uuid: Option<Uuid>,
    pub support_position: Option<Vec3>,
    pub vertical_velocity: f32,
}

impl Default for CharacterController {
    fn default() -> Self {
        Self {
            radius: 0.5,
            height: 2.0,
            max_slope_degrees: 45.0,
            max_step_height: 0.3,
            support_distance: 0.1,
            jump_height: 1.5,
            state: CharacterState::default(),
        }
    }
}

impl CharacterController {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.radius.is_finite()
            || self.radius <= 0.0
            || !self.height.is_finite()
            || self.height < self.radius * 2.0
            || !self.max_slope_degrees.is_finite()
            || !(0.0..90.0).contains(&self.max_slope_degrees)
            || !self.max_step_height.is_finite()
            || self.max_step_height < 0.0
            || !self.support_distance.is_finite()
            || self.support_distance < 0.0
            || !self.jump_height.is_finite()
            || self.jump_height < 0.0
        {
            return Err("Invalid character controller settings");
        }
        Ok(())
    }
}
