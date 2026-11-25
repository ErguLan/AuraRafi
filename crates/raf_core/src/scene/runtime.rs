use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VariableValue {
    Bool(bool),
    Number(f32),
    Text(String),
    /// Stable authoring reference. None is deliberately unassigned, never a name fallback.
    Entity(Option<uuid::Uuid>),
}

impl Default for VariableValue {
    fn default() -> Self {
        Self::Number(0.0)
    }
}

impl VariableValue {
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::Bool(_) => "Bool",
            Self::Number(_) => "Number",
            Self::Text(_) => "Text",
            Self::Entity(_) => "Entity",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct SceneVariable {
    pub name: String,
    #[serde(default)]
    pub value: VariableValue,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum RigidBodyType {
    Static,
    Dynamic,
    Kinematic,
}

impl Default for RigidBodyType {
    fn default() -> Self {
        Self::Static
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigidBody {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub body_type: RigidBodyType,
    #[serde(default = "default_true")]
    pub use_gravity: bool,
    #[serde(default)]
    pub velocity: Vec3,
    #[serde(default = "default_mass")]
    pub mass: f32,
    #[serde(default = "default_damping")]
    pub damping: f32,
    #[serde(default)]
    pub is_trigger: bool,
    /// Opt-in controller body which remains simulated in the lightweight mode.
    #[serde(default)]
    pub is_character: bool,
    /// Angular velocity in world-space radians per second.
    #[serde(default)]
    pub angular_velocity: Vec3,
    #[serde(default = "default_friction")]
    pub friction: f32,
    #[serde(default)]
    pub restitution: f32,
    #[serde(default = "default_damping")]
    pub angular_damping: f32,
    #[serde(default = "default_mass")]
    pub gravity_scale: f32,
    #[serde(default)]
    pub translation_locks: [bool; 3],
    #[serde(default)]
    pub rotation_locks: [bool; 3],
    #[serde(skip)]
    pub force: Vec3,
    #[serde(skip)]
    pub torque: Vec3,
    #[serde(skip)]
    pub impulse: Vec3,
    #[serde(skip)]
    pub angular_impulse: Vec3,
    #[serde(skip)]
    pub sleeping: bool,
    #[serde(skip)]
    pub sleep_timer: f32,
    #[serde(skip)]
    pub sleep_gravity: Vec3,
}

impl Default for RigidBody {
    fn default() -> Self {
        Self {
            enabled: false,
            body_type: RigidBodyType::Static,
            use_gravity: true,
            velocity: Vec3::ZERO,
            mass: default_mass(),
            damping: default_damping(),
            is_trigger: false,
            is_character: false,
            angular_velocity: Vec3::ZERO,
            friction: default_friction(),
            restitution: 0.0,
            angular_damping: default_damping(),
            gravity_scale: 1.0,
            translation_locks: [false; 3],
            rotation_locks: [false; 3],
            force: Vec3::ZERO,
            torque: Vec3::ZERO,
            impulse: Vec3::ZERO,
            angular_impulse: Vec3::ZERO,
            sleeping: false,
            sleep_timer: 0.0,
            sleep_gravity: Vec3::ZERO,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AudioBus {
    Master,
    Music,
    #[default]
    Sfx,
    Voice,
    Ambient,
}

impl AudioBus {
    pub const ALL: [Self; 5] = [
        Self::Master,
        Self::Music,
        Self::Sfx,
        Self::Voice,
        Self::Ambient,
    ];

    pub const fn slug(self) -> &'static str {
        match self {
            Self::Master => "master",
            Self::Music => "music",
            Self::Sfx => "sfx",
            Self::Voice => "voice",
            Self::Ambient => "ambient",
        }
    }

    pub fn from_slug(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "master" => Some(Self::Master),
            "music" => Some(Self::Music),
            "sfx" | "effects" => Some(Self::Sfx),
            "voice" => Some(Self::Voice),
            "ambient" => Some(Self::Ambient),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioSource {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub clip: String,
    #[serde(default)]
    pub autoplay: bool,
    #[serde(default)]
    pub looping: bool,
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Mixer destination. Existing scenes without this field use SFX.
    #[serde(default)]
    pub bus: AudioBus,
    /// When enabled, playback gain follows the entity's world position and the
    /// active camera listener. This is the 3D link between sound and a block.
    #[serde(default)]
    pub spatial: bool,
    #[serde(default = "default_audio_min_distance")]
    pub min_distance: f32,
    #[serde(default = "default_audio_max_distance")]
    pub max_distance: f32,
    #[serde(default = "default_audio_rolloff")]
    pub rolloff: f32,
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            enabled: false,
            clip: String::new(),
            autoplay: false,
            looping: false,
            volume: default_volume(),
            bus: AudioBus::Sfx,
            spatial: false,
            min_distance: default_audio_min_distance(),
            max_distance: default_audio_max_distance(),
            rolloff: default_audio_rolloff(),
        }
    }
}

impl AudioSource {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.volume.is_finite() || !(0.0..=1.0).contains(&self.volume) {
            return Err("Audio volume must be within 0..1.");
        }
        if !self.min_distance.is_finite()
            || !self.max_distance.is_finite()
            || self.min_distance <= 0.0
            || self.max_distance <= self.min_distance
            || self.max_distance > 10_000.0
        {
            return Err("Audio distances require 0 < min < max <= 10000.");
        }
        if !self.rolloff.is_finite() || !(0.1..=8.0).contains(&self.rolloff) {
            return Err("Audio rolloff must be within 0.1..8.");
        }
        Ok(())
    }

    pub fn spatial_gain(&self, distance: f32) -> f32 {
        if !self.spatial || !distance.is_finite() {
            return 1.0;
        }
        if distance <= self.min_distance {
            return 1.0;
        }
        if distance >= self.max_distance {
            return 0.0;
        }
        let normalized = (distance - self.min_distance)
            / (self.max_distance - self.min_distance).max(f32::EPSILON);
        (1.0 - normalized).clamp(0.0, 1.0).powf(self.rolloff)
    }
}

fn default_audio_min_distance() -> f32 {
    1.0
}

fn default_audio_max_distance() -> f32 {
    20.0
}

fn default_audio_rolloff() -> f32 {
    1.0
}

fn default_true() -> bool {
    true
}

fn default_mass() -> f32 {
    1.0
}

fn default_friction() -> f32 {
    0.5
}

fn default_damping() -> f32 {
    0.06
}

fn default_volume() -> f32 {
    1.0
}

#[cfg(test)]
mod audio_tests {
    use super::*;

    #[test]
    fn spatial_curve_and_bus_defaults_are_bounded() {
        let mut source = AudioSource::default();
        assert_eq!(source.bus, AudioBus::Sfx);
        assert!(!source.spatial);
        source.spatial = true;
        source.min_distance = 1.0;
        source.max_distance = 10.0;
        assert_eq!(source.spatial_gain(0.5), 1.0);
        assert_eq!(source.spatial_gain(10.0), 0.0);
        assert!(source.spatial_gain(5.0) > 0.0);
        assert!(source.validate().is_ok());
    }

    #[test]
    fn invalid_audio_distances_are_rejected() {
        let mut source = AudioSource::default();
        source.max_distance = 1.0;
        assert!(source.validate().is_err());
        source.max_distance = 20.0;
        source.volume = 1.5;
        assert!(source.validate().is_err());
    }
}
