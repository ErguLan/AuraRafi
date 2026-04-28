use glam::Vec3;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum VariableValue {
    Bool(bool),
    Number(f32),
    Text(String),
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
}

impl Default for AudioSource {
    fn default() -> Self {
        Self {
            enabled: false,
            clip: String::new(),
            autoplay: false,
            looping: false,
            volume: default_volume(),
        }
    }
}

fn default_true() -> bool {
    true
}

fn default_mass() -> f32 {
    1.0
}

fn default_damping() -> f32 {
    0.06
}

fn default_volume() -> f32 {
    1.0
}