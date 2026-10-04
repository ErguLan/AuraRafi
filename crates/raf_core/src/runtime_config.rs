//! Persisted local runtime policy. Neither server transport nor editor state lives here.
use glam::Vec3;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeLaunchMode {
    #[default]
    SeparateWindow,
    SameWindow,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum RuntimeErrorPolicy {
    #[default]
    Pause,
    DisableScript,
    Stop,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RuntimePreferences {
    pub launch_mode: RuntimeLaunchMode,
    pub max_instances: u32,
    pub window_size: [u32; 2],
    pub fps_limit: u32,
    pub script_budget_ms: u32,
    pub script_operation_limit: u64,
    pub hot_reload: bool,
    pub diagnostics: bool,
    pub error_policy: RuntimeErrorPolicy,
}
impl Default for RuntimePreferences {
    fn default() -> Self {
        Self {
            launch_mode: RuntimeLaunchMode::SeparateWindow,
            max_instances: 1,
            window_size: [960, 540],
            fps_limit: 60,
            script_budget_ms: 10,
            script_operation_limit: 100_000,
            hot_reload: true,
            diagnostics: true,
            error_policy: RuntimeErrorPolicy::Pause,
        }
    }
}
impl RuntimePreferences {
    pub fn normalized(&self) -> Self {
        let mut settings = self.clone();
        settings.max_instances = settings.max_instances.clamp(1, 4);
        settings.window_size = [
            settings.window_size[0].clamp(640, 1920),
            settings.window_size[1].clamp(360, 1080),
        ];
        settings.fps_limit = settings.fps_limit.clamp(15, 120);
        settings.script_budget_ms = settings.script_budget_ms.clamp(1, 100);
        settings.script_operation_limit = settings.script_operation_limit.clamp(1000, 1_000_000);
        settings
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInputAction {
    pub name: String,
    pub keys: Vec<String>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ProjectRuntimeSettings {
    /// Explicit world/session selection. None means the current unsaved world.
    pub startup_session: Option<Uuid>,
    /// Only this configured camera is used. Missing references never select another node.
    pub active_camera: Option<Uuid>,
    pub fixed_hz: u32,
    pub max_entities: usize,
    pub input_actions: Vec<RuntimeInputAction>,
}
impl Default for ProjectRuntimeSettings {
    fn default() -> Self {
        Self {
            startup_session: None,
            active_camera: None,
            fixed_hz: 60,
            max_entities: 10_000,
            input_actions: Vec::new(),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameCamera {
    pub orthographic: bool,
    pub fov_degrees: f32,
    pub near: f32,
    pub far: f32,
    pub ortho_scale: f32,
}
impl Default for GameCamera {
    fn default() -> Self {
        Self {
            orthographic: false,
            fov_degrees: 60.0,
            near: 0.1,
            far: 1000.0,
            ortho_scale: 10.0,
        }
    }
}
impl GameCamera {
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.fov_degrees.is_finite()
            || !(1.0..179.0).contains(&self.fov_degrees)
            || !self.near.is_finite()
            || !self.far.is_finite()
            || self.near <= 0.0
            || self.far <= self.near
            || self.far > 100_000.0
            || !self.ortho_scale.is_finite()
            || self.ortho_scale <= 0.0
        {
            return Err("Invalid camera lens: FOV 1..179, 0 < near < far <= 100000, scale > 0");
        }
        Ok(())
    }
}
/// Resolved pose in SI units. Render hosts translate it to their private camera type.
#[derive(Debug, Clone, PartialEq)]
pub struct RuntimeCameraPose {
    pub position: Vec3,
    pub forward: Vec3,
    pub up: Vec3,
    pub projection: GameCamera,
}
